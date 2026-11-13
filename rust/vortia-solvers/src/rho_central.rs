//! Transient compressible flow of a perfect gas with the central-upwind
//! scheme of Kurganov and Tadmor, explicit in time.
//!
//! The engine's counterpart of OpenFOAM's `rhoCentralFoam` (OpenFOAM v1912,
//! GPL-3.0-or-later), inviscid, with a fixed time step or, under
//! `ddtSchemes localEuler`, a time step of its own for each cell: the densities of
//! mass, momentum and energy are advanced from the fluxes of the two
//! directed face values the limited `reconstruct(...)` schemes give, and
//! the velocity, the energy, the temperature and the pressure follow from
//! them, in the order the original takes them. It reads `T`, `U` and `p`
//! (and `rho` when present) and writes them back with `rho`.

use vortia_core::Vec3;
use vortia_fields::{Data, Location, ScalarField, VectorField};
use vortia_fv::ops;
use vortia_fv::reconstruct::{Reconstruction, interpolate};
use vortia_fv::smooth::smooth;
use vortia_fv::vector::solution_directions;
use vortia_fv::{Boundary, ScalarBc, VectorBc, VectorBoundary};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, VELOCITY, check_dimensions};
use crate::control::RunControl;
use crate::error::{Result, SolverError};
use crate::ico::{Step, switch};
use crate::rho_simple::{WithBoundary, calculated_field, mul, mul_b, read_calculated};
use crate::thermo::{DENSITY, Gas, PRESSURE, Thermo};

/// `max(a, b)` as OpenFOAM writes it, which keeps `b` when `a` is not
/// greater, NaN included.
fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// `min(a, b)`, the same way.
fn min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

/// `magSqr(v)`.
fn mag_sqr(v: Vec3) -> f64 {
    v.x * v.x + v.y * v.y + v.z * v.z
}

/// Which central flux the case asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FluxScheme {
    Kurganov,
    Tadmor,
}

/// The schemes `rhoCentralFoam` reads.
struct Schemes {
    flux: FluxScheme,
    rho: Reconstruction,
    u: Reconstruction,
    t: Reconstruction,
}

impl Schemes {
    fn read(case: &Case) -> Result<Self> {
        let flux = match case.schemes.word("fluxScheme") {
            None | Some("Kurganov") => FluxScheme::Kurganov,
            Some("Tadmor") => FluxScheme::Tadmor,
            Some(other) => {
                return Err(SolverError::Setup(format!(
                    "fluxScheme '{other}' is not Tadmor or Kurganov"
                )));
            }
        };
        let read = |key: &str, vector: bool| -> Result<Reconstruction> {
            let words = case.scheme("interpolationSchemes", key)?;
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            Ok(Reconstruction::from_scheme(&words, vector)?)
        };
        Ok(Self {
            flux,
            rho: read("reconstruct(rho)", false)?,
            u: read("reconstruct(U)", true)?,
            t: read("reconstruct(T)", false)?,
        })
    }
}

/// How the equations advance in time.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stepping {
    /// One `deltaT` everywhere.
    Euler,
    /// `localEuler`: each cell has the time step its Courant number allows,
    /// from `setRDeltaT.H`.
    Local {
        max_co: f64,
        max_delta_t: f64,
        smoothing: f64,
    },
}

impl Stepping {
    fn read(case: &Case) -> Result<Self> {
        let mut found = None;
        for key in ["ddt(rho)", "ddt(rhoU)", "ddt(rhoE)"] {
            let word = case.scheme("ddtSchemes", key)?.first().cloned();
            let this = match word.as_deref() {
                Some("Euler") => false,
                Some("localEuler") => true,
                other => {
                    return Err(SolverError::Setup(format!(
                        "ddt scheme '{}' is not supported: use Euler or localEuler",
                        other.unwrap_or("")
                    )));
                }
            };
            if found.is_some_and(|f| f != this) {
                return Err(SolverError::Setup(
                    "ddt(rho), ddt(rhoU) and ddt(rhoE) must all be Euler or all localEuler".into(),
                ));
            }
            found = Some(this);
        }
        if found != Some(true) {
            return Ok(Self::Euler);
        }
        let max_co = case.scalar("maxCo")?;
        if !(max_co > 0.0 && max_co.is_finite()) {
            return Err(SolverError::Setup("maxCo must be positive".into()));
        }
        let max_delta_t = case.control.scalar("maxDeltaT").unwrap_or(1.0e15);
        if max_delta_t.is_nan() || max_delta_t <= 0.0 {
            return Err(SolverError::Setup("maxDeltaT must be positive".into()));
        }
        Ok(Self::Local {
            max_co,
            max_delta_t,
            smoothing: case.control.scalar("rDeltaTSmoothingCoeff").unwrap_or(0.02),
        })
    }
}

/// `setRDeltaT.H`: the reciprocal time step of each cell, from the sums of
/// its faces' largest fluxes, smoothed so neighbours stay within a ratio.
fn local_r_delta_t(
    mesh: &PolyMesh,
    volumes: &[f64],
    sum_amax: &[f64],
    (max_co, max_delta_t, smoothing): (f64, f64, f64),
) -> Vec<f64> {
    let floor = 1.0 / max_delta_t;
    let mut r: Vec<f64> = sum_amax
        .iter()
        .zip(volumes)
        .map(|(&s, &v)| max(floor, s / ((2.0 * max_co) * v)))
        .collect();
    if smoothing < 1.0 {
        smooth(mesh, &mut r, smoothing);
    }
    r
}

/// The pressure, its conditions and its value on the boundary faces.
struct Pressure {
    file: ScalarField,
    bc: Boundary,
}

impl Pressure {
    fn read(start: &std::path::Path, mesh: &PolyMesh) -> Result<(Self, Vec<f64>)> {
        let file = ScalarField::read(start.join("p"))?;
        check_dimensions("p", &file.dimensions, PRESSURE)?;
        let bc = Boundary::from_field(&file, mesh)?;
        if let Some(c) = bc.conditions.iter().find(|c| {
            !matches!(
                c,
                ScalarBc::FixedValue(_) | ScalarBc::ZeroGradient | ScalarBc::Empty
            )
        }) {
            return Err(SolverError::Setup(format!(
                "p: condition {c:?} is not supported yet; use fixedValue or zeroGradient"
            )));
        }
        let cells = file
            .internal
            .expand(mesh.n_cells())
            .ok_or_else(|| SolverError::Setup("p does not fit the mesh".into()))?;
        Ok((Self { file, bc }, cells))
    }
}

/// The fluxes of one face, from its two directed states.
struct FaceFlux {
    /// Mass.
    phi: f64,
    /// Momentum, with the pressure.
    phi_up: Vec3,
    /// Energy, with the pressure work.
    phi_ep: f64,
    /// The larger of the two estimated fluxes, for the Courant number.
    amax_sf: f64,
}

/// One side of a face: the reconstructed fields.
#[derive(Clone, Copy)]
struct Side {
    rho: f64,
    rho_u: Vec3,
    r_psi: f64,
    c: f64,
    e: f64,
}

/// `rhoCentralFoam`'s flux of one face from its `pos` and `neg` sides, as
/// `rhoCentralFoam.C` assembles it.
fn face_flux(flux: FluxScheme, sf: Vec3, mag_sf: f64, pos: Side, neg: Side) -> FaceFlux {
    let u_pos = pos.rho_u / pos.rho;
    let u_neg = neg.rho_u / neg.rho;
    let p_pos = pos.rho * pos.r_psi;
    let p_neg = neg.rho * neg.r_psi;
    let phiv_pos = u_pos.dot(sf);
    let phiv_neg = u_neg.dot(sf);
    let c_sf_pos = pos.c * mag_sf;
    let c_sf_neg = neg.c * mag_sf;
    let ap = max(max(phiv_pos + c_sf_pos, phiv_neg + c_sf_neg), 0.0);
    let am = min(min(phiv_pos - c_sf_pos, phiv_neg - c_sf_neg), 0.0);
    let mut a_pos = ap / (ap - am);
    let amax_sf = max(am.abs(), ap.abs());
    let mut a_sf = am * a_pos;
    if flux == FluxScheme::Tadmor {
        a_sf = -0.5 * amax_sf;
        a_pos = 0.5;
    }
    let a_neg = 1.0 - a_pos;
    let phiv_pos = phiv_pos * a_pos;
    let phiv_neg = phiv_neg * a_neg;
    let aphiv_pos = phiv_pos - a_sf;
    let aphiv_neg = phiv_neg + a_sf;
    FaceFlux {
        phi: aphiv_pos * pos.rho + aphiv_neg * neg.rho,
        phi_up: (pos.rho_u * aphiv_pos + neg.rho_u * aphiv_neg)
            + sf * (a_pos * p_pos + a_neg * p_neg),
        phi_ep: aphiv_pos * (pos.rho * (pos.e + 0.5 * mag_sqr(u_pos)) + p_pos)
            + aphiv_neg * (neg.rho * (neg.e + 0.5 * mag_sqr(u_neg)) + p_neg)
            + a_sf * p_pos
            - a_sf * p_neg,
        amax_sf: max(aphiv_pos.abs(), aphiv_neg.abs()),
    }
}

/// `solve(fvm::ddt(x) + fvc::div(flux))` with Euler's `ddt` and the
/// diagonal solver: `(x⁰ / Δt V − V ∇·φ) / (V / Δt)`, in OpenFOAM's order.
fn advance(old: &[f64], div: &[f64], volumes: &[f64], r_delta_t: &[f64]) -> Vec<f64> {
    old.iter()
        .zip(div)
        .zip(volumes)
        .zip(r_delta_t)
        .map(|(((&x, &d), &v), &r)| {
            let source = r * x * v - v * d;
            source / (r * v)
        })
        .collect()
}

/// [`advance`] for each component that is solved for (not the direction of
/// an empty patch), which keeps its old value otherwise.
fn advance_vector(
    mesh: &PolyMesh,
    fv: &vortia_fv::FvGeometry,
    valid: [bool; 3],
    old: &[Vec3],
    flux: &[Vec3],
    r_delta_t: &[f64],
) -> Vec<Vec3> {
    let solve = |ok: bool, pick: fn(&Vec3) -> f64| -> Option<Vec<f64>> {
        ok.then(|| {
            let old: Vec<f64> = old.iter().map(pick).collect();
            let flux: Vec<f64> = flux.iter().map(pick).collect();
            advance(
                &old,
                &ops::surface_integrate(mesh, fv, &flux),
                fv.volumes(),
                r_delta_t,
            )
        })
    };
    let x = solve(valid[0], |v| v.x);
    let y = solve(valid[1], |v| v.y);
    let z = solve(valid[2], |v| v.z);
    let at = |s: &Option<Vec<f64>>, i: usize, old: f64| {
        s.as_ref().and_then(|v| v.get(i)).copied().unwrap_or(old)
    };
    old.iter()
        .enumerate()
        .map(|(i, v)| Vec3::new(at(&x, i, v.x), at(&y, i, v.y), at(&z, i, v.z)))
        .collect()
}

/// Runs the case and returns what each step did. `log` receives the lines
/// OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    let gas = Gas::read_with(case, true)?;
    if gas.energy != crate::thermo::Energy::Internal {
        return Err(SolverError::Setup(
            "rhoCentralFoam solves the sensible internal energy: use energy sensibleInternalEnergy"
                .into(),
        ));
    }
    if gas.mu() != 0.0 {
        return Err(SolverError::Setup(
            "rhoCentralFoam with a viscous gas is not supported yet; use mu 0".into(),
        ));
    }
    let stepping = Stepping::read(case)?;
    if stepping == Stepping::Euler && switch(&case.control, "adjustTimeStep") == Some(true) {
        return Err(SolverError::Setup(
            "rhoCentralFoam with adjustTimeStep is not supported yet; use a fixed deltaT".into(),
        ));
    }
    let schemes = Schemes::read(case)?;
    let run = RunControl::read(case)?;
    let start = case.dir.join(&run.start_name);

    let (mut p, mut p_cells) = Pressure::read(&start, &mesh)?;
    let mut p_b = p.bc.face_values(&mesh, &p_cells, &fv.delta_coeffs);
    let mut thermo = Thermo::read(gas, &mesh, &start, (&p_cells, &p_b))?;
    if !thermo.has_plain_conditions() {
        return Err(SolverError::Setup(
            "T: rhoCentralFoam supports fixedValue and zeroGradient only".into(),
        ));
    }

    let mut u_field = VectorField::read(start.join("U"))?;
    check_dimensions("U", &u_field.dimensions, VELOCITY)?;
    let u_bc = VectorBoundary::from_field(&u_field, &mesh)?;
    if let Some(c) = u_bc.conditions.iter().find(|c| {
        !matches!(
            c,
            VectorBc::FixedValue(_) | VectorBc::ZeroGradient | VectorBc::Empty
        )
    }) {
        return Err(SolverError::Setup(format!(
            "U: condition {c:?} is not supported yet; use fixedValue or zeroGradient"
        )));
    }
    let mut u = u_field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("U does not fit the mesh".into()))?;
    let mut u_b = u_bc.face_values(&mesh, &u);

    let (mut rho, mut rho_b): WithBoundary =
        if start.join("rho").is_file() || start.join("rho.gz").is_file() {
            read_calculated(&mesh, &start.join("rho"), DENSITY)?
        } else {
            (mul(&p_cells, &thermo.psi), mul_b(&p_b, &thermo.psi_b))
        };
    // rhoU = rho U and rhoE = rho (e + ½|U|²), whose cells the equations
    // advance and whose boundary values the loop sets from the others.
    let mut rho_u: Vec<Vec3> = rho.iter().zip(&u).map(|(&r, &v)| v * r).collect();
    let mut rho_u_b: Vec<Option<Vec3>> = rho_b
        .iter()
        .zip(&u_b)
        .map(|(r, v)| r.zip(*v).map(|(r, v)| v * r))
        .collect();
    let mut rho_e: Vec<f64> = rho
        .iter()
        .zip(&thermo.he)
        .zip(&u)
        .map(|((&r, &e), &v)| r * (e + 0.5 * mag_sqr(v)))
        .collect();

    let valid = solution_directions(&mesh, &fv);
    let gamma = thermo.gas.cp_by_cpv();
    let n_internal = mesh.n_internal_faces();
    let n_faces = mesh.n_faces();
    let mut r_delta_t = vec![1.0 / run.delta_t; mesh.n_cells()];
    let volumes = fv.volumes();
    let total_volume: f64 = volumes.iter().sum();

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = run.name(step);

        // The directed face values of the primitive fields.
        let r_psi: Vec<f64> = thermo.psi.iter().map(|&s| 1.0 / s).collect();
        let r_psi_b: Vec<Option<f64>> = thermo.psi_b.iter().map(|s| s.map(|s| 1.0 / s)).collect();
        let c: Vec<f64> = r_psi.iter().map(|&r| (gamma * r).sqrt()).collect();
        let c_b: Vec<Option<f64>> = r_psi_b
            .iter()
            .map(|r| r.map(|r| (gamma * r).sqrt()))
            .collect();
        let scalar = |scheme: Reconstruction, cells: &[f64], boundary: &[Option<f64>]| {
            let side = |dir: f64| {
                let w = scheme.weights(&mesh, &fv, dir, cells, boundary);
                interpolate(&mesh, &w, cells, boundary)
            };
            (side(1.0), side(-1.0))
        };
        let (rho_pos, rho_neg) = scalar(schemes.rho, &rho, &rho_b);
        let (r_psi_pos, r_psi_neg) = scalar(schemes.t, &r_psi, &r_psi_b);
        let (c_pos, c_neg) = scalar(schemes.t, &c, &c_b);
        let (e_pos, e_neg) = scalar(schemes.t, &thermo.he, &thermo.he_b);
        let vector = |dir: f64| {
            let w = schemes.u.weights_vector(&mesh, &fv, dir, &rho_u, &rho_u_b);
            interpolate(&mesh, &w, &rho_u, &rho_u_b)
        };
        let (rho_u_pos, rho_u_neg) = (vector(1.0), vector(-1.0));

        let mut phi = vec![0.0; n_faces];
        let mut phi_up = vec![Vec3::default(); n_faces];
        let mut phi_ep = vec![0.0; n_faces];
        let mut amax_sf = vec![0.0; n_faces];
        for f in 0..n_faces {
            if f >= n_internal && rho_b.get(f - n_internal).copied().flatten().is_none() {
                continue;
            }
            let at = |v: &[f64]| v.get(f).copied().unwrap_or(0.0);
            let vec_at = |v: &[Vec3]| v.get(f).copied().unwrap_or_default();
            let pos = Side {
                rho: at(&rho_pos),
                rho_u: vec_at(&rho_u_pos),
                r_psi: at(&r_psi_pos),
                c: at(&c_pos),
                e: at(&e_pos),
            };
            let neg = Side {
                rho: at(&rho_neg),
                rho_u: vec_at(&rho_u_neg),
                r_psi: at(&r_psi_neg),
                c: at(&c_neg),
                e: at(&e_neg),
            };
            let sf = fv.geo.face_areas.get(f).copied().unwrap_or_default();
            let mag_sf = fv.mag_sf.get(f).copied().unwrap_or(0.0);
            let out = face_flux(schemes.flux, sf, mag_sf, pos, neg);
            if let (Some(a), Some(b), Some(c), Some(d)) = (
                phi.get_mut(f),
                phi_up.get_mut(f),
                phi_ep.get_mut(f),
                amax_sf.get_mut(f),
            ) {
                (*a, *b, *c, *d) = (out.phi, out.phi_up, out.phi_ep, out.amax_sf);
            }
        }

        // centralCourantNo.H
        let sum_amax = surface_sum(&mesh, &amax_sf);
        let max_co = sum_amax
            .iter()
            .zip(volumes)
            .map(|(&s, &v)| s / v)
            .fold(f64::NEG_INFINITY, f64::max);
        let mean_co = 0.5 * (sum_amax.iter().sum::<f64>() / total_volume) * run.delta_t;
        let co = 0.5 * max_co * run.delta_t;
        log(&format!("Mean and max Courant Numbers = {mean_co} {co}\n"));
        if let Stepping::Local {
            max_co,
            max_delta_t,
            smoothing,
        } = stepping
        {
            r_delta_t =
                local_r_delta_t(&mesh, volumes, &sum_amax, (max_co, max_delta_t, smoothing));
            let low = r_delta_t.iter().copied().fold(f64::INFINITY, f64::min);
            let high = r_delta_t.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            log(&format!(
                "Flow time scale min/max = {}, {}\n",
                1.0 / high,
                1.0 / low
            ));
        }
        log(&format!("\nTime = {name}\n\n"));

        // Density, momentum, then velocity.
        let rho_new = advance(
            &rho,
            &ops::surface_integrate(&mesh, &fv, &phi),
            volumes,
            &r_delta_t,
        );
        let rho_u_new = advance_vector(&mesh, &fv, valid, &rho_u, &phi_up, &r_delta_t);
        rho_u = rho_u_new;
        rho = rho_new;
        u = rho_u.iter().zip(&rho).map(|(&m, &r)| m / r).collect();
        u_b = u_bc.face_values(&mesh, &u);
        // The momentum on the boundary takes the density the step began
        // with, which is not updated until the pressure is.
        rho_u_b = rho_b
            .iter()
            .zip(&u_b)
            .map(|(r, v)| r.zip(*v).map(|(r, v)| v * r))
            .collect();

        // Energy, temperature and pressure.
        let rho_e_new = advance(
            &rho_e,
            &ops::surface_integrate(&mesh, &fv, &phi_ep),
            volumes,
            &r_delta_t,
        );
        rho_e = rho_e_new;
        let e: Vec<f64> = rho_e
            .iter()
            .zip(&rho)
            .zip(&u)
            .map(|((&re, &r), &v)| re / r - 0.5 * mag_sqr(v))
            .collect();
        thermo.set_energy(&mesh, e, &p_cells, &p_b)?;
        p_cells = rho.iter().zip(&thermo.psi).map(|(&r, &s)| r / s).collect();
        p_b = p.bc.face_values(&mesh, &p_cells, &fv.delta_coeffs);
        rho_b = mul_b(&thermo.psi_b, &p_b);
        let _ = Location::Cells;

        for line in ["rho", "rhoU", "rhoE"] {
            log(&format!(
                "diagonal:  Solving for {line}, Initial residual = 0, Final residual = 0, No Iterations 0\n"
            ));
        }
        let written = run.writes(step);
        if written {
            u_field.internal = Data::NonUniform(u.clone());
            u_field.drop_zero_gradient_values();
            u_field.store_patch_values(&mesh, &[], &u_bc.face_values(&mesh, &u));
            u_field.write(&case.dir, &name)?;
            p.file.internal = Data::NonUniform(p_cells.clone());
            p.file.drop_zero_gradient_values();
            p.file.store_patch_values(&mesh, &[], &p_b);
            p.file.write(&case.dir, &name)?;
            thermo.to_write(&mesh).write(&case.dir, &name)?;
            calculated_field(&mesh, "rho", Location::Cells, DENSITY, &rho, &rho_b)
                .write(&case.dir, &name)?;
            if stepping != Stepping::Euler {
                let owner = mesh.owner();
                let on_faces: Vec<Option<f64>> = (n_internal..n_faces)
                    .map(|f| owner.get(f).and_then(|&o| r_delta_t.get(o)).copied())
                    .collect();
                calculated_field(
                    &mesh,
                    "rDeltaT",
                    Location::Cells,
                    [0, 0, -1, 0, 0, 0, 0],
                    &r_delta_t,
                    &on_faces,
                )
                .write(&case.dir, &name)?;
            }
        }
        steps.push(Step {
            time: name,
            solves: Vec::new(),
            written,
        });
    }
    log("End\n");
    let _ = (&mut p.bc, PatchKind::Empty);
    Ok(steps)
}

/// `fvc::surfaceSum`: every cell's sum of its faces' values, the boundary
/// faces included.
fn surface_sum(mesh: &PolyMesh, face: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; mesh.n_cells()];
    for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        let x = face.get(f).copied().unwrap_or(0.0);
        if let Some(v) = out.get_mut(o) {
            *v += x;
        }
        if let Some(v) = out.get_mut(n) {
            *v += x;
        }
    }
    for patch in mesh.patches() {
        if patch.kind == PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            if let (Some(&o), Some(&x)) = (mesh.owner().get(f), face.get(f))
                && let Some(v) = out.get_mut(o)
            {
                *v += x;
            }
        }
    }
    out
}
