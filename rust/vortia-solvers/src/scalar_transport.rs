//! Convection and diffusion of a passive scalar by a given flux:
//! `∂T/∂t + ∇·(φ T) = ∇·(D_T ∇T)`.
//!
//! The engine's counterpart of OpenFOAM's `scalarTransportFoam` (OpenFOAM,
//! GPL-3.0-or-later). It reads `T` and the face flux `phi` from the start
//! time, or builds `phi` from `U` when there is none, as OpenFOAM does.

use vortia_core::Vec3;
use vortia_fields::{Data, ScalarField, VectorField};
use vortia_fv::fvgeom::transform;
use vortia_fv::ops::{self, Convection};
use vortia_fv::{Boundary, FvGeometry, LduMatrix};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, VELOCITY, VOLUME_FLUX, check_dimensions};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::ico::flux_field;
use crate::laplacian::{Ddt, Step, diffusivity, sn_grad};

/// Runs the case and returns what each time step did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    let dt_coeff = diffusivity(case)?;

    let ddt = Ddt::read(case, "ddt(T)")?;
    let words = case.scheme("divSchemes", "div(phi,T)")?;
    let convection =
        Convection::from_scheme(&words.iter().map(String::as_str).collect::<Vec<_>>())?;
    let sn_grad = sn_grad(case, "laplacian(DT,T)", "T")?;
    let (solver, controls) = LinearSolver::read(case, "T")?;
    let n_corr = case.non_orthogonal_correctors("SIMPLE");
    let run = RunControl::read(case)?;

    let start = case.dir.join(&run.start_name);
    let mut field = ScalarField::read(start.join("T"))?;
    let bc = Boundary::from_field(&field, &mesh)?;
    let mut t = field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("T does not fit the mesh".into()))?;
    let (phi, flow) = flux(&mesh, &fv, &start)?;

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = run.name(step);
        log(&format!("Time = {name}\n\n"));
        let old = t.clone();
        let mut solves = Vec::new();
        for _ in 0..=n_corr {
            let mut m = LduMatrix::new(&mesh);
            if ddt == Ddt::Euler {
                ops::ddt_euler(&mut m, &fv, &old, run.delta_t)?;
            }
            // First, so its round-off falls where OpenFOAM's does.
            if convection.bounded {
                ops::bounded_correction(&mut m, &mesh, &fv, &phi);
            }
            ops::convection(&mut m, &mesh, &fv, &bc, &phi, convection.interpolation)?;
            ops::diffusion(&mut m, &mesh, &fv, &bc, dt_coeff, sn_grad, &t)?;
            let perf = solver.solve(&m, &mut t, &controls)?;
            log(&solve_line("T", &perf));
            solves.push(perf);
        }
        let written = run.writes(step);
        if written {
            field.internal = Data::NonUniform(t.clone());
            field.write(&case.dir, &name)?;
            // The flow goes with it, as OpenFOAM writes it, so a run can
            // restart from any time it wrote.
            match &flow {
                Flow::Phi(f) => f.write(&case.dir, &name)?,
                Flow::U(u) => {
                    u.write(&case.dir, &name)?;
                    flux_field(&mesh, &phi).write(&case.dir, &name)?;
                }
            }
        }
        steps.push(Step {
            time: name,
            solves,
            written,
        });
    }
    log("End\n");
    Ok(steps)
}

/// Where the flux came from, to be written back beside `T`.
enum Flow {
    Phi(ScalarField),
    U(VectorField),
}

/// The face flux, internal faces then boundary faces in patch order: `phi`
/// when the start time has one, else `U` interpolated to the faces.
fn flux(mesh: &PolyMesh, fv: &FvGeometry, start: &std::path::Path) -> Result<(Vec<f64>, Flow)> {
    let path = start.join("phi");
    if path.is_file() || start.join("phi.gz").is_file() {
        let phi = ScalarField::read(&path)?;
        return Ok((face_values(mesh, &phi)?, Flow::Phi(phi)));
    }
    let u = VectorField::read(start.join("U"))?;
    Ok((flux_of(mesh, fv, &u)?, Flow::U(u)))
}

pub(crate) fn face_values(mesh: &PolyMesh, phi: &ScalarField) -> Result<Vec<f64>> {
    phi.check_against(mesh)?;
    check_dimensions("phi", &phi.dimensions, VOLUME_FLUX)?;
    let mut out = phi
        .internal
        .expand(mesh.n_internal_faces())
        .ok_or_else(|| SolverError::Setup("phi does not fit the mesh".into()))?;
    for patch in mesh.patches() {
        if patch.kind == PatchKind::Empty {
            out.extend(std::iter::repeat_n(0.0, patch.size));
            continue;
        }
        let values = phi
            .patch_field(patch)
            .and_then(|p| p.value.as_ref())
            .and_then(|v| v.expand(patch.size))
            .ok_or_else(|| {
                SolverError::Setup(format!("phi has no values on patch '{}'", patch.name))
            })?;
        out.extend(values);
    }
    Ok(out)
}

/// `linearInterpolate(U) & Sf`, with each patch's own velocity.
fn flux_of(mesh: &PolyMesh, fv: &FvGeometry, u: &VectorField) -> Result<Vec<f64>> {
    u.check_against(mesh)?;
    check_dimensions("U", &u.dimensions, VELOCITY)?;
    let cells = u
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("U does not fit the mesh".into()))?;
    let sf = &fv.geo.face_areas;
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or_default();
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        // `Sf & (w·(P − N) + N)`: OpenFOAM's `dotInterpolate`, whose
        // round-off the solve then sees.
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        let face = (o - n) * w + n;
        out.push(face.dot(sf.get(f).copied().unwrap_or_default()));
    }
    for patch in mesh.patches() {
        let pf = u.patch_field(patch);
        let kind = pf.map_or("", |p| p.kind.as_str());
        let wedge = fv.wedge_face_rotation(patch);
        let given = pf
            .and_then(|p| p.value.as_ref())
            .and_then(|v| v.expand(patch.size));
        for (i, f) in patch.faces().enumerate() {
            let s = sf.get(f).copied().unwrap_or_default();
            let inside = cell(mesh.owner().get(f));
            let face = match (kind, &given) {
                ("empty", _) => {
                    out.push(0.0);
                    continue;
                }
                ("noSlip", _) => Vec3::default(),
                ("zeroGradient", _) => inside,
                // No flow through the side: the cell's velocity minus its
                // normal part.
                ("wedge", _) => transform(&wedge, inside),
                ("slip" | "symmetry" | "symmetryPlane", _) => {
                    let n = s / s.norm().max(f64::MIN_POSITIVE);
                    inside - n * inside.dot(n)
                }
                (_, Some(v)) => v.get(i).copied().unwrap_or_default(),
                (other, None) => {
                    return Err(SolverError::Setup(format!(
                        "U on patch '{}' ('{other}') has no value to build phi from",
                        patch.name
                    )));
                }
            };
            out.push(face.dot(s));
        }
    }
    Ok(out)
}
