//! Transient incompressible laminar flow with the PISO algorithm:
//! `∂U/∂t + ∇·(φ U) − ∇·(ν∇U) = −∇p`, `∇·U = 0`.
//!
//! The engine's counterpart of OpenFOAM's `icoFoam` (OpenFOAM,
//! GPL-3.0-or-later), with its pieces in the same order: the momentum
//! predictor, then `nCorrectors` pressure corrections, each forming `HbyA`,
//! its flux with the `ddtCorr` coupling term, the pressure equation and the
//! corrected flux and velocity. It reads `U`, `p` (and `phi` when present)
//! from the start time and `nu` from `constant/transportProperties`, and
//! writes `U`, `p` and `phi`.

use vortia_core::Vec3;
use vortia_fields::{Data, Location, PatchField, ScalarField, VectorField};
use vortia_foamfile::{Dictionary, Value};
use vortia_fv::ops::{self, Interpolation, SnGrad};
use vortia_fv::vector::{dot_interpolate, solution_directions};
use vortia_fv::{
    Boundary, FvGeometry, LduMatrix, Performance, ScalarBc, VectorBoundary, VectorMatrix,
};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, time_name};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::laplacian::{Ddt, transport_property};
use crate::scalar_transport::face_values;

/// What one time step did.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub time: String,
    /// Each solve in order, with the field it solved (`Ux`, `p`, …).
    pub solves: Vec<(String, Performance)>,
    pub written: bool,
}

/// The `PISO` dictionary of `fvSolution`.
#[derive(Debug, Clone, PartialEq)]
struct Piso {
    n_correctors: usize,
    n_non_orth: usize,
    momentum_predictor: bool,
    ref_cell: usize,
    ref_value: f64,
}

impl Piso {
    fn read(case: &Case, n_cells: usize) -> Result<Self> {
        let d = case.solution.dict("PISO").cloned().unwrap_or_default();
        let count = |k: &str, def: usize| {
            d.label(k)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(def)
                .min(100)
        };
        if d.get("pRefPoint").is_some() {
            return Err(SolverError::Setup(
                "PISO pRefPoint is not supported yet; use pRefCell".into(),
            ));
        }
        let ref_cell = match d.label("pRefCell") {
            None => 0,
            Some(v) => usize::try_from(v).unwrap_or(usize::MAX),
        };
        if ref_cell >= n_cells {
            return Err(SolverError::Setup(format!(
                "pRefCell {} is not a cell of a {n_cells}-cell mesh",
                d.label("pRefCell").unwrap_or_default()
            )));
        }
        Ok(Self {
            n_correctors: count("nCorrectors", 1).max(1),
            n_non_orth: count("nNonOrthogonalCorrectors", 0),
            momentum_predictor: switch(&d, "momentumPredictor").unwrap_or(true),
            ref_cell,
            ref_value: d.scalar("pRefValue").unwrap_or(0.0),
        })
    }
}

/// An OpenFOAM switch: `yes`/`no`, `on`/`off`, `true`/`false`, `1`/`0`.
fn switch(d: &Dictionary, key: &str) -> Option<bool> {
    match d.value(key)? {
        Value::Word(w) => match w.as_str() {
            "yes" | "on" | "true" | "y" | "t" => Some(true),
            "no" | "off" | "false" | "n" | "f" | "none" => Some(false),
            _ => None,
        },
        Value::Label(v) => Some(*v != 0),
        _ => None,
    }
}

/// The schemes `icoFoam` asks for, checked against what the engine has.
struct Schemes {
    convection: Interpolation,
    momentum_sn_grad: SnGrad,
    pressure_sn_grad: SnGrad,
}

impl Schemes {
    fn read(case: &Case) -> Result<Self> {
        if Ddt::read(case, "ddt(U)")? != Ddt::Euler {
            return Err(SolverError::Setup(
                "icoFoam is transient: ddt(U) must be Euler".into(),
            ));
        }
        let words = |section: &str, key: &str| -> Result<Vec<String>> { case.scheme(section, key) };
        let convection = Interpolation::from_scheme(&as_str(&words("divSchemes", "div(phi,U)")?))?;
        let sn = |key: &str| -> Result<SnGrad> {
            let w = words("laplacianSchemes", key)?;
            let s = SnGrad::from_scheme(&as_str(&w))?;
            if s == SnGrad::Corrected {
                return Err(SolverError::Setup(format!(
                    "laplacian scheme '{}' for {key} is not supported yet by icoFoam; \
                     use Gauss linear orthogonal or uncorrected",
                    w.join(" ")
                )));
            }
            Ok(s)
        };
        let grad = |field: &str| -> Result<()> {
            let w = words("gradSchemes", &format!("grad({field})"))?;
            if w != ["Gauss", "linear"] {
                return Err(SolverError::Setup(format!(
                    "grad scheme '{}' is not supported yet; use Gauss linear",
                    w.join(" ")
                )));
            }
            Ok(())
        };
        grad("p")?;
        Ok(Self {
            convection,
            momentum_sn_grad: sn("laplacian(nu,U)")?,
            pressure_sn_grad: sn("laplacian((1|A(U)),p)")?,
        })
    }
}

fn as_str(w: &[String]) -> Vec<&str> {
    w.iter().map(String::as_str).collect()
}

/// Runs the case and returns what each time step did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let mesh = PolyMesh::read_case(&case.dir)?;
    let fv = FvGeometry::new(&mesh);
    let nu = transport_property(case, "nu")?;
    let schemes = Schemes::read(case)?;
    let piso = Piso::read(case, mesh.n_cells())?;
    let (u_solver, u_controls) = LinearSolver::read(case, "U")?;
    let (p_solver, p_controls) = LinearSolver::read(case, "p")?;
    let (p_final_solver, p_final_controls) = LinearSolver::read(case, "pFinal")?;
    let run = RunControl::read(case)?;

    let start = case.dir.join(&run.start_name);
    let mut u_field = VectorField::read(start.join("U"))?;
    let mut p_field = ScalarField::read(start.join("p"))?;
    let u_bc = VectorBoundary::from_field(&u_field, &mesh)?;
    let p_bc = Boundary::from_field(&p_field, &mesh)?;
    let mut u = u_field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("U does not fit the mesh".into()))?;
    let mut p = p_field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("p does not fit the mesh".into()))?;
    let mut phi = if start.join("phi").is_file() || start.join("phi.gz").is_file() {
        face_values(&mesh, &ScalarField::read(start.join("phi"))?)?
    } else {
        dot_interpolate(&mesh, &fv, &u, &u_bc.face_values(&mesh, &u))
    };
    let need_reference = !p_bc
        .conditions
        .iter()
        .any(|c| matches!(c, ScalarBc::FixedValue(_)));
    let valid = solution_directions(&mesh, &fv);
    let names = ["Ux", "Uy", "Uz"];

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = time_name(run.time(step));
        log(&format!("Time = {name}\n\n"));
        let u_old = u.clone();
        let phi_old = phi.clone();
        let mut solves = Vec::new();

        let mut u_eqn = VectorMatrix::ddt_euler(&mesh, &fv, &u_old, run.delta_t)?;
        u_eqn.add(&VectorMatrix::convection(
            &mesh,
            &fv,
            &u_bc,
            &phi,
            schemes.convection,
        )?);
        u_eqn.sub(&VectorMatrix::laplacian(
            &mesh,
            &fv,
            &u_bc,
            nu,
            schemes.momentum_sn_grad,
        )?);

        if piso.momentum_predictor {
            let grad_p = ops::grad(&mesh, &fv, &p_bc, &p);
            let extra: Vec<Vec3> = grad_p
                .iter()
                .zip(fv.volumes())
                .map(|(&g, &v)| (g * -1.0) * v)
                .collect();
            let perfs = u_eqn.solve_segregated(&mesh, &mut u, Some(&extra), valid, |m, x, _| {
                u_solver.solve(m, x, &u_controls)
            })?;
            for (cmpt, perf) in perfs {
                let field = names.get(cmpt).copied().unwrap_or("U");
                log(&solve_line(field, &perf));
                solves.push((field.to_owned(), perf));
            }
        }

        let ddt_corr = ddt_corr(&mesh, &fv, &u_bc, &u_old, &phi_old, run.delta_t);
        for corr in 1..=piso.n_correctors {
            let r_au: Vec<f64> = u_eqn.a(&mesh, &fv).iter().map(|&a| 1.0 / a).collect();
            let h = u_eqn.h(&mesh, &fv, &u, valid);
            let hbya: Vec<Vec3> = h.iter().zip(&r_au).map(|(&h, &r)| h * r).collect();
            let hbya_b = constrained_boundary(&mesh, &u_bc, &u, &hbya);
            let r_au_f = ops::interpolate_linear(&mesh, &fv, &r_au, &extrapolated(&mesh, &r_au));
            let mut phi_hbya = dot_interpolate(&mesh, &fv, &hbya, &hbya_b);
            for ((f, &r), &c) in phi_hbya.iter_mut().zip(&r_au_f).zip(&ddt_corr) {
                *f += r * c;
            }
            if need_reference {
                adjust_phi(&mesh, &u_bc, &mut phi_hbya)?;
            }

            for non_orth in 0..=piso.n_non_orth {
                let mut m = LduMatrix::new(&mesh);
                ops::laplacian(&mut m, &mesh, &fv, &p_bc, &r_au_f, schemes.pressure_sn_grad)?;
                let div = ops::surface_integrate(&mesh, &fv, &phi_hbya);
                for ((s, &d), &v) in m.source.iter_mut().zip(&div).zip(fv.volumes()) {
                    *s += v * d;
                }
                if need_reference {
                    ops::set_reference(&mut m, piso.ref_cell, piso.ref_value);
                }
                let last = non_orth == piso.n_non_orth;
                let perf = if corr == piso.n_correctors && last {
                    p_final_solver.solve(&m, &mut p, &p_final_controls)?
                } else {
                    p_solver.solve(&m, &mut p, &p_controls)?
                };
                log(&solve_line("p", &perf));
                solves.push(("p".to_owned(), perf));
                if last {
                    let flux = ops::laplacian_flux(
                        &mesh,
                        &fv,
                        &p_bc,
                        &r_au_f,
                        schemes.pressure_sn_grad,
                        &p,
                    );
                    for ((f, &a), &b) in phi.iter_mut().zip(&phi_hbya).zip(&flux) {
                        *f = a - b;
                    }
                }
            }

            let grad_p = ops::grad(&mesh, &fv, &p_bc, &p);
            for (((uc, &hb), &r), &g) in u.iter_mut().zip(&hbya).zip(&r_au).zip(&grad_p) {
                *uc = hb - g * r;
            }
        }

        let written = run.writes(step);
        if written {
            u_field.internal = Data::NonUniform(u.clone());
            u_field.write(&case.dir, &name)?;
            p_field.internal = Data::NonUniform(p.clone());
            p_field.write(&case.dir, &name)?;
            flux_field(&mesh, &phi).write(&case.dir, &name)?;
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

/// A cell field's own values on every boundary face (`None` on empty faces):
/// OpenFOAM's `extrapolatedCalculated`.
fn extrapolated(mesh: &PolyMesh, cells: &[f64]) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(mesh.n_faces().saturating_sub(mesh.n_internal_faces()));
    for patch in mesh.patches() {
        for f in patch.faces() {
            out.push(if patch.kind == PatchKind::Empty {
                None
            } else {
                mesh.owner().get(f).and_then(|&c| cells.get(c)).copied()
            });
        }
    }
    out
}

/// `HbyA` on the boundary: the velocity where the velocity is fixed
/// (OpenFOAM's `constrainHbyA`), else the cell's own value.
fn constrained_boundary(
    mesh: &PolyMesh,
    bc: &VectorBoundary,
    u: &[Vec3],
    hbya: &[Vec3],
) -> Vec<Option<Vec3>> {
    let fixed = bc.face_values(mesh, u);
    let mut out = Vec::with_capacity(fixed.len());
    let mut k = 0;
    for (i, patch) in mesh.patches().iter().enumerate() {
        for f in patch.faces() {
            let own = mesh.owner().get(f).and_then(|&c| hbya.get(c)).copied();
            let v = fixed.get(k).copied().flatten();
            out.push(match (patch.kind == PatchKind::Empty, bc.fixes_value(i)) {
                (true, _) => None,
                (false, true) => v,
                (false, false) => own,
            });
            k += 1;
        }
    }
    out
}

/// OpenFOAM's `ddtCorr(U, phi)` for Euler: how far the old flux is from the
/// flux of the old velocity, per unit time, scaled down where the two
/// disagree strongly and zero on patches that fix the velocity.
fn ddt_corr(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &VectorBoundary,
    u_old: &[Vec3],
    phi_old: &[f64],
    dt: f64,
) -> Vec<f64> {
    const SMALL: f64 = 1e-15;
    let r_dt = 1.0 / dt;
    let interpolated = dot_interpolate(mesh, fv, u_old, &bc.face_values(mesh, u_old));
    let mut out: Vec<f64> = phi_old
        .iter()
        .zip(&interpolated)
        .map(|(&phi, &ui)| {
            let corr = phi - ui;
            let coeff = 1.0 - (corr.abs() / (phi.abs() + SMALL)).min(1.0);
            coeff * r_dt * corr
        })
        .collect();
    for (i, patch) in mesh.patches().iter().enumerate() {
        if bc.fixes_value(i) || patch.kind == PatchKind::Empty {
            for f in patch.faces() {
                if let Some(v) = out.get_mut(f) {
                    *v = 0.0;
                }
            }
        }
    }
    out
}

/// Scales the outflow through patches that do not fix the velocity so as
/// much leaves as enters (OpenFOAM's `adjustPhi`), when the pressure has no
/// fixed level to let the imbalance out.
fn adjust_phi(mesh: &PolyMesh, bc: &VectorBoundary, phi: &mut [f64]) -> Result<()> {
    const SMALL: f64 = 1e-15;
    const VSMALL: f64 = 1e-300;
    let (mut mass_in, mut fixed_out, mut adjustable_out) = (0.0, 0.0, 0.0);
    for (i, patch) in mesh.patches().iter().enumerate() {
        if patch.kind == PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            let v = phi.get(f).copied().unwrap_or(0.0);
            if v < 0.0 {
                mass_in -= v;
            } else if bc.fixes_value(i) {
                fixed_out += v;
            } else {
                adjustable_out += v;
            }
        }
    }
    let total: f64 = VSMALL
        + phi
            .iter()
            .take(mesh.n_internal_faces())
            .map(|v| v.abs())
            .sum::<f64>();
    let mut mass_corr = 1.0;
    let mag_adjustable: f64 = f64::abs(adjustable_out);
    if mag_adjustable > VSMALL && mag_adjustable / total > SMALL {
        mass_corr = (mass_in - fixed_out) / adjustable_out;
    } else if (fixed_out - mass_in).abs() / total > 1e-8 {
        return Err(SolverError::Setup(format!(
            "continuity error cannot be removed by adjusting the outflow: inflow {mass_in}, \
             fixed outflow {fixed_out}, adjustable outflow {adjustable_out}; check the \
             velocity boundary conditions"
        )));
    }
    for (i, patch) in mesh.patches().iter().enumerate() {
        if bc.fixes_value(i) || patch.kind == PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            if let Some(v) = phi.get_mut(f)
                && *v > 0.0
            {
                *v *= mass_corr;
            }
        }
    }
    Ok(())
}

/// `phi` as the `surfaceScalarField` OpenFOAM writes.
fn flux_field(mesh: &PolyMesh, phi: &[f64]) -> ScalarField {
    let n_internal = mesh.n_internal_faces();
    let boundary = mesh
        .patches()
        .iter()
        .map(|patch| {
            let empty = patch.kind == PatchKind::Empty;
            PatchField {
                key: patch.name.clone(),
                kind: if empty { "empty" } else { "calculated" }.to_owned(),
                value: (!empty).then(|| {
                    Data::NonUniform(
                        patch
                            .faces()
                            .map(|f| phi.get(f).copied().unwrap_or(0.0))
                            .collect(),
                    )
                }),
                settings: Dictionary::new(),
            }
        })
        .collect();
    let dims = [0, 3, -1, 0, 0, 0, 0]
        .into_iter()
        .map(Value::Label)
        .collect();
    ScalarField {
        name: "phi".into(),
        location: Location::Faces,
        dimensions: Value::Dimensions(dims),
        internal: Data::NonUniform(phi.iter().take(n_internal).copied().collect()),
        boundary,
        boundary_directives: Dictionary::new(),
        extra: Dictionary::new(),
    }
}
