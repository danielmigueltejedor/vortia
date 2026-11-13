//! Transient compressible flow of a perfect gas with the central-upwind
//! scheme of Kurganov and Tadmor, explicit in time.
//!
//! The engine's counterpart of OpenFOAM's `rhoCentralFoam` (OpenFOAM v1912,
//! GPL-3.0-or-later), laminar, with a fixed time step or, under
//! `ddtSchemes localEuler`, a time step of its own for each cell: the densities of
//! mass, momentum and energy are advanced from the fluxes of the two
//! directed face values the limited `reconstruct(...)` schemes give, and
//! the velocity, the energy, the temperature and the pressure follow from
//! them, in the order the original takes them. A viscous gas adds the
//! implicit diffusion of the velocity and the energy and the work of the
//! viscous stress, as the original does. It reads `T`, `U` and `p`
//! (and `rho` when present) and writes them back with `rho`.

use vortia_core::Vec3;
use vortia_fields::{Data, Location, ScalarField, VectorField};
use vortia_fv::limited::{self, GradScheme};
use vortia_fv::ops::{self, SnGrad};
use vortia_fv::reconstruct::{Reconstruction, interpolate};
use vortia_fv::smooth::smooth;
use vortia_fv::stress::{self, dev2_stress, dot_interpolate_tensor};
use vortia_fv::vector::solution_directions;
use vortia_fv::{
    Boundary, Controls, FvGeometry, ScalarBc, ScalarMatrix, VectorBc, VectorBoundary, VectorMatrix,
    correction,
};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, VELOCITY, check_dimensions};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::ico::{Step, switch};
use crate::rans::{Model, face_diffusivity};
use crate::rho_simple::{WithBoundary, calculated_field, mul, mul_b, read_calculated};
use crate::spalart::{self, SpalartAllmaras};
use crate::thermo::{DENSITY, Flow, Gas, PRESSURE, Thermo};

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
    /// The gradients the limiters read, per reconstructed field: `rho`,
    /// `rPsi`, `c`, `e` and the momentum (`rhoU` itself for a `V` scheme,
    /// else its squared magnitude).
    grads: [GradScheme; 5],
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
        let u = read("reconstruct(U)", true)?;
        let momentum = match u {
            Reconstruction::Limited { vector: true, .. } => "grad(rhoU)",
            _ => "grad(magSqr(rhoU))",
        };
        let grad = |key: &str| case.grad_scheme(key);
        Ok(Self {
            flux,
            rho: read("reconstruct(rho)", false)?,
            u,
            t: read("reconstruct(T)", false)?,
            grads: [
                grad("grad(rho)")?,
                grad("grad(rPsi)")?,
                grad("grad(c)")?,
                grad("grad(e)")?,
                grad(momentum)?,
            ],
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

/// What a viscous gas adds: the schemes and the solvers of the velocity and
/// the energy equations.
struct Viscous {
    velocity_grad: GradScheme,
    energy_grad: GradScheme,
    /// The Laplacians' gradients, each with the coefficient of a `limited`
    /// scheme.
    momentum_sn_grad: (SnGrad, Option<f64>),
    energy_sn_grad: (SnGrad, Option<f64>),
    /// `snGrad(U)`, of the work of the viscous stress.
    face_sn_grad: (SnGrad, Option<f64>),
    u_solver: (LinearSolver, Controls),
    e_solver: (LinearSolver, Controls),
}

impl Viscous {
    fn read(case: &Case) -> Result<Self> {
        let stress = case.scheme("divSchemes", "div(tauMC)")?;
        if stress != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "div scheme '{}' for div(tauMC) is not supported yet; use Gauss linear",
                stress.join(" ")
            )));
        }
        let mu = case.scheme("interpolationSchemes", "interpolate(muEff)")?;
        if mu != ["linear"] {
            return Err(SolverError::Setup(format!(
                "interpolation scheme '{}' for muEff is not supported yet; use linear",
                mu.join(" ")
            )));
        }
        Ok(Self {
            velocity_grad: case.grad_scheme("grad(U)")?,
            energy_grad: case.grad_scheme("grad(e)")?,
            momentum_sn_grad: case.limited_laplacian_sn_grad("laplacian(muEff,U)")?,
            energy_sn_grad: case.limited_laplacian_sn_grad("laplacian(alphaEff,e)")?,
            face_sn_grad: case.limited_face_sn_grad("snGrad(U)")?,
            u_solver: LinearSolver::read(case, "U")?,
            e_solver: LinearSolver::read(case, "e")?,
        })
    }
}

/// The diagonal and the source of `fvm::ddt(ρ, ψ) − fvc::ddt(ρ, ψ)` for
/// Euler or local Euler: `fvm::ddt`'s, then `V` times `fvc::ddt` added to the
/// source, in OpenFOAM's order. `old_*` are the step's start.
fn ddt_correction<T>(
    rho: &[f64],
    old_rho: &[f64],
    old_x: &[T],
    x: &[T],
    volumes: &[f64],
    r_delta_t: &[f64],
) -> (Vec<f64>, Vec<T>)
where
    T: std::ops::Mul<f64, Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Add<Output = T>
        + Copy,
{
    let mut diag = Vec::with_capacity(rho.len());
    let mut source = Vec::with_capacity(rho.len());
    for (((((&r, &r0), &x0), &x), &v), &rd) in rho
        .iter()
        .zip(old_rho)
        .zip(old_x)
        .zip(x)
        .zip(volumes)
        .zip(r_delta_t)
    {
        diag.push(rd * r * v);
        // fvm::ddt's source, then the cell's `fvc::ddt` taken off it.
        let fvm = x0 * (rd * r0) * v;
        source.push(fvm + (x * r - x0 * r0) * rd * v);
    }
    (diag, source)
}

/// What the viscous terms read of the start of the step.
struct Start {
    rho: Vec<f64>,
    u: Vec<Vec3>,
    he: Vec<f64>,
    /// `Sf & tauMC` on every face.
    tau_dot: Vec<Vec3>,
    /// `∇·tauMC` in every cell.
    div_tau: Vec<Vec3>,
    /// `muEff` in the cells and on the boundary faces.
    mu_eff: Vec<f64>,
    mu_eff_b: Vec<Option<f64>>,
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
                ScalarBc::FixedValue(_)
                    | ScalarBc::ZeroGradient
                    | ScalarBc::Reflect
                    | ScalarBc::Empty
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
    /// `a_pos U_pos + a_neg U_neg`, the velocity the stress works with.
    u_face: Vec3,
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
        u_face: u_pos * a_pos + u_neg * a_neg,
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

/// [`advance`] with `V ∇·σ` taken from the right side as well:
/// `solve(ddt(x) + div(flux) − div(σ))`.
fn advance_with(
    old: &[f64],
    div: &[f64],
    extra: &[f64],
    volumes: &[f64],
    r_delta_t: &[f64],
) -> Vec<f64> {
    old.iter()
        .zip(div)
        .zip(extra)
        .zip(volumes)
        .zip(r_delta_t)
        .map(|((((&x, &d), &e), &v), &r)| {
            let source = (r * x * v - v * d) + v * e;
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
    let viscous = if gas.mu() > 0.0 {
        Some(Viscous::read(case)?)
    } else {
        None
    };
    let turbulence_coeffs = match Model::read(case)? {
        Model::Laminar => None,
        Model::SpalartAllmaras(ras) => {
            if viscous.is_none() {
                return Err(SolverError::Setup(
                    "SpalartAllmaras needs a viscous gas: set transport mu".into(),
                ));
            }
            Some(ras.dict("SpalartAllmarasCoeffs").cloned().unwrap_or(ras))
        }
        _ => {
            return Err(SolverError::Setup(
                "rhoCentralFoam runs laminar flow or SpalartAllmaras only".into(),
            ));
        }
    };
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
            VectorBc::FixedValue(_)
                | VectorBc::ZeroGradient
                | VectorBc::Wedge { .. }
                | VectorBc::Empty
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
    let mut turbulence = turbulence_coeffs
        .as_ref()
        .map(|coeffs| SpalartAllmaras::read(case, &mesh, &fv, &start, Some(coeffs), None))
        .transpose()?;
    let gamma = thermo.gas.cp_by_cpv();
    let n_internal = mesh.n_internal_faces();
    let n_faces = mesh.n_faces();
    let mut r_delta_t = vec![1.0 / run.delta_t; mesh.n_cells()];
    if let Some(t) = turbulence.as_mut() {
        t.validate(
            &mesh,
            &fv,
            (&u, &u_bc),
            &spalart::Gas {
                rho: &rho,
                rho_b: &rho_b,
                rho_old: &rho,
                mu: &thermo.mu,
                mu_b: &thermo.mu_b,
                r_delta_t: &r_delta_t,
            },
        );
    }
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
        let scalar =
            |scheme: Reconstruction, cells: &[f64], boundary: &[Option<f64>], grad: GradScheme| {
                let side = |dir: f64| {
                    let w = scheme.weights(&mesh, &fv, dir, cells, boundary, grad);
                    interpolate(&mesh, &w, cells, boundary)
                };
                (side(1.0), side(-1.0))
            };
        let (rho_pos, rho_neg) = scalar(schemes.rho, &rho, &rho_b, schemes.grads[0]);
        let (r_psi_pos, r_psi_neg) = scalar(schemes.t, &r_psi, &r_psi_b, schemes.grads[1]);
        let (c_pos, c_neg) = scalar(schemes.t, &c, &c_b, schemes.grads[2]);
        let (e_pos, e_neg) = scalar(schemes.t, &thermo.he, &thermo.he_b, schemes.grads[3]);
        let vector = |dir: f64| {
            let w = schemes
                .u
                .weights_vector(&mesh, &fv, dir, &rho_u, &rho_u_b, schemes.grads[4]);
            interpolate(&mesh, &w, &rho_u, &rho_u_b)
        };
        let (rho_u_pos, rho_u_neg) = (vector(1.0), vector(-1.0));

        let mut phi = vec![0.0; n_faces];
        let mut phi_up = vec![Vec3::default(); n_faces];
        let mut phi_ep = vec![0.0; n_faces];
        let mut amax_sf = vec![0.0; n_faces];
        let mut u_face = vec![Vec3::default(); n_faces];
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
            if let Some(x) = u_face.get_mut(f) {
                *x = out.u_face;
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

        // tauMC and what the step started from, for the viscous terms.
        let viscous_start = viscous.as_ref().map(|v| {
            let (mu_eff, mu_eff_b) = match &turbulence {
                Some(t) => t.mu_eff(&spalart::Gas {
                    rho: &rho,
                    rho_b: &rho_b,
                    rho_old: &rho,
                    mu: &thermo.mu,
                    mu_b: &thermo.mu_b,
                    r_delta_t: &r_delta_t,
                }),
                None => (thermo.mu.clone(), thermo.mu_b.clone()),
            };
            let (tau, tau_b) =
                dev2_stress(v.velocity_grad, &mesh, &fv, &u_bc, &u, &mu_eff, &mu_eff_b);
            let tau_dot = dot_interpolate_tensor(&mesh, &fv, &tau, &tau_b);
            let component =
                |pick: fn(&Vec3) -> f64| -> Vec<f64> { tau_dot.iter().map(pick).collect() };
            let div_tau = [
                ops::surface_integrate(&mesh, &fv, &component(|v| v.x)),
                ops::surface_integrate(&mesh, &fv, &component(|v| v.y)),
                ops::surface_integrate(&mesh, &fv, &component(|v| v.z)),
            ];
            let div_tau: Vec<Vec3> = (0..mesh.n_cells())
                .map(|c| {
                    let at = |d: &Vec<f64>| d.get(c).copied().unwrap_or(0.0);
                    Vec3::new(at(&div_tau[0]), at(&div_tau[1]), at(&div_tau[2]))
                })
                .collect();
            Start {
                rho: rho.clone(),
                u: u.clone(),
                he: thermo.he.clone(),
                tau_dot,
                div_tau,
                mu_eff,
                mu_eff_b,
            }
        });

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
        // `rho.correctBoundaryConditions()` leaves the faces of the other
        // patches alone (`calculated`) but takes the cell's density on a
        // wedge, whose condition is a constraint.
        for patch in mesh.patches() {
            if patch.kind != PatchKind::Wedge {
                continue;
            }
            for f in patch.faces() {
                let cell = mesh.owner().get(f).and_then(|&c| rho.get(c)).copied();
                if let (Some(slot), Some(cell)) = (rho_b.get_mut(f - n_internal), cell) {
                    *slot = Some(cell);
                }
            }
        }
        u = rho_u.iter().zip(&rho).map(|(&m, &r)| m / r).collect();
        u_b = u_bc.face_values(&mesh, &u);
        // The momentum on the boundary takes the density the step began
        // with, which is not updated until the pressure is.
        rho_u_b = rho_b
            .iter()
            .zip(&u_b)
            .map(|(r, v)| r.zip(*v).map(|(r, v)| v * r))
            .collect();

        if let (Some(v), Some(start)) = (&viscous, &viscous_start) {
            let eqn = momentum_equation(&mesh, &fv, &u_bc, v, start, (&rho, &u), &r_delta_t)?;
            let mut solved = u.clone();
            let perfs = eqn.solve_segregated(&mesh, &mut solved, None, valid, |m, x, _| {
                v.u_solver.0.solve(m, x, &v.u_solver.1)
            })?;
            for (cmpt, perf) in perfs {
                let field = ["Ux", "Uy", "Uz"].get(cmpt).copied().unwrap_or("U");
                log(&solve_line(field, &perf));
            }
            u = solved;
            u_b = u_bc.face_values(&mesh, &u);
            rho_u = rho.iter().zip(&u).map(|(&r, &x)| x * r).collect();
            rho_u_b = rho_b
                .iter()
                .zip(&u_b)
                .map(|(r, x)| r.zip(*x).map(|(r, x)| x * r))
                .collect();
        }

        // Energy, temperature and pressure.
        let rho_e_new = match (&viscous, &viscous_start) {
            (Some(v), Some(start)) => {
                // The work of the viscous stress: σ·U on every face.
                let (g, _) = stress::grad(v.velocity_grad, &mesh, &fv, &u_bc, &u);
                let sn = correction::face_sn_grad_vector(&mesh, &fv, &u_bc, &u, v.face_sn_grad, &g);
                let mu_f = face_diffusivity(&mesh, &fv, &start.mu_eff, &start.mu_eff_b);
                let sigma_dot_u: Vec<f64> = (0..n_faces)
                    .map(|f| {
                        let mu = mu_f.get(f).copied().unwrap_or(0.0);
                        let mag = fv.mag_sf.get(f).copied().unwrap_or(0.0);
                        let sn = sn.get(f).copied().unwrap_or_default();
                        let tau = start.tau_dot.get(f).copied().unwrap_or_default();
                        let u = u_face.get(f).copied().unwrap_or_default();
                        (sn * (mu * mag) + tau).dot(u)
                    })
                    .collect();
                advance_with(
                    &rho_e,
                    &ops::surface_integrate(&mesh, &fv, &phi_ep),
                    &ops::surface_integrate(&mesh, &fv, &sigma_dot_u),
                    volumes,
                    &r_delta_t,
                )
            }
            _ => advance(
                &rho_e,
                &ops::surface_integrate(&mesh, &fv, &phi_ep),
                volumes,
                &r_delta_t,
            ),
        };
        rho_e = rho_e_new;
        let e: Vec<f64> = rho_e
            .iter()
            .zip(&rho)
            .zip(&u)
            .map(|((&re, &r), &v)| re / r - 0.5 * mag_sqr(v))
            .collect();
        thermo.set_energy(&mesh, e, &p_cells, &p_b)?;
        if let (Some(v), Some(start)) = (&viscous, &viscous_start) {
            diffuse_energy(
                &mesh,
                &fv,
                v,
                &mut thermo,
                start,
                (&rho, &p_cells, &p_b),
                &Flow {
                    u_b: &u_b,
                    phi: &phi,
                },
                &r_delta_t,
                turbulence.as_ref().map(|t| t.alphat()),
                &mut log,
            )?;
            rho_e = rho
                .iter()
                .zip(&thermo.he)
                .zip(&u)
                .map(|((&r, &e), &v)| r * (e + 0.5 * mag_sqr(v)))
                .collect();
        }
        p_cells = rho.iter().zip(&thermo.psi).map(|(&r, &s)| r / s).collect();
        p_b = p.bc.face_values(&mesh, &p_cells, &fv.delta_coeffs);
        rho_b = mul_b(&thermo.psi_b, &p_b);
        let _ = Location::Cells;
        if let (Some(t), Some(start)) = (turbulence.as_mut(), &viscous_start) {
            t.correct(
                &mesh,
                &fv,
                (&u, &u_bc),
                &phi,
                &spalart::Gas {
                    rho: &rho,
                    rho_b: &rho_b,
                    rho_old: &start.rho,
                    mu: &thermo.mu,
                    mu_b: &thermo.mu_b,
                    r_delta_t: &r_delta_t,
                },
                &mut log,
            )?;
        }

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
            if let Some(t) = &turbulence {
                t.write(&mesh, &case.dir, &name)?;
            }
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

/// `fvm::ddt(ρ, U) − fvc::ddt(ρ, U) − fvm::laplacian(μ, U) − fvc::div(tauMC)`
/// with the velocity the inviscid update gave and the density it ended on.
#[allow(clippy::too_many_arguments)]
fn momentum_equation(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    u_bc: &VectorBoundary,
    v: &Viscous,
    start: &Start,
    (rho, u): (&[f64], &[Vec3]),
    r_delta_t: &[f64],
) -> Result<VectorMatrix> {
    let (diag, source) = ddt_correction(rho, &start.rho, &start.u, u, fv.volumes(), r_delta_t);
    let mut eqn = VectorMatrix::new(mesh);
    eqn.ldu.diag = diag;
    eqn.source = source;
    let mu_f = face_diffusivity(mesh, fv, &start.mu_eff, &start.mu_eff_b);
    let mut lap = VectorMatrix::laplacian(
        mesh,
        fv,
        u_bc,
        &mu_f,
        u,
        v.momentum_sn_grad.0.without_correction(),
    )?;
    if v.momentum_sn_grad.0 == SnGrad::Corrected {
        let (g, _) = stress::grad(v.velocity_grad, mesh, fv, u_bc, u);
        let mut corr = correction::sn_grad_correction_vector(mesh, fv, &g);
        if let Some(k) = v.momentum_sn_grad.1 {
            corr = correction::limit_correction(mesh, fv, u, &corr, k, Vec3::norm);
        }
        let flux = correction::scale_by_gamma_mag_sf(fv, &mu_f, &corr);
        correction::subtract_divergence(&mut lap.source, mesh, &flux);
    }
    eqn.sub(&lap);
    for ((s, &d), &vol) in eqn.source.iter_mut().zip(&start.div_tau).zip(fv.volumes()) {
        *s = *s + d * vol;
    }
    Ok(eqn)
}

/// `fvm::ddt(ρ, e) − fvc::ddt(ρ, e) − fvm::laplacian(αEff, e)`, solved, and
/// the temperature that follows.
#[allow(clippy::too_many_arguments)]
fn diffuse_energy(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    v: &Viscous,
    thermo: &mut Thermo,
    start: &Start,
    (rho, p, p_b): (&[f64], &[f64], &[Option<f64>]),
    flow: &Flow<'_>,
    r_delta_t: &[f64],
    alphat: Option<&crate::rans::ModelField>,
    log: &mut impl FnMut(&str),
) -> Result<()> {
    let he_bc = thermo.energy_boundary(mesh, fv, p_b, flow);
    let (diag, source) = ddt_correction(
        rho,
        &start.rho,
        &start.he,
        &thermo.he,
        fv.volumes(),
        r_delta_t,
    );
    let mut eqn = ScalarMatrix::new(mesh);
    eqn.ldu.diag = diag;
    eqn.ldu.source = source;
    let cp_by_cpv = thermo.gas.cp_by_cpv();
    let turbulent = |a: f64, t: Option<f64>| cp_by_cpv * (a + t.unwrap_or(0.0));
    let alpha_eff: Vec<f64> = thermo
        .alpha
        .iter()
        .enumerate()
        .map(|(i, &a)| turbulent(a, alphat.and_then(|t| t.cells.get(i).copied())))
        .collect();
    let alpha_eff_b: Vec<Option<f64>> = thermo
        .alpha_b
        .iter()
        .enumerate()
        .map(|(i, a)| {
            a.map(|a| turbulent(a, alphat.and_then(|t| t.faces.get(i).copied().flatten())))
        })
        .collect();
    let alpha_f = face_diffusivity(mesh, fv, &alpha_eff, &alpha_eff_b);
    let mut lap = ScalarMatrix::laplacian(
        mesh,
        fv,
        &he_bc,
        &alpha_f,
        v.energy_sn_grad.0.without_correction(),
    )?;
    if v.energy_sn_grad.0 == SnGrad::Corrected {
        let g = limited::grad(v.energy_grad, mesh, fv, &thermo.he, &thermo.he_b);
        let mut corr = correction::sn_grad_correction(mesh, fv, &g);
        if let Some(k) = v.energy_sn_grad.1 {
            corr = correction::limit_correction(mesh, fv, &thermo.he, &corr, k, f64::abs);
        }
        let flux = correction::scale_by_gamma_mag_sf(fv, &alpha_f, &corr);
        correction::subtract_divergence(&mut lap.ldu.source, mesh, &flux);
    }
    eqn.sub(&lap);
    let perf = eqn.solve(mesh, &mut thermo.he, |m, x| {
        v.e_solver.0.solve(m, x, &v.e_solver.1)
    })?;
    log(&solve_line("e", &perf));
    thermo.he_b = he_bc.face_values(mesh, &thermo.he, &fv.delta_coeffs);
    thermo.correct(mesh, p, p_b)
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
