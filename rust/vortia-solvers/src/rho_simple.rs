//! Steady compressible flow of a perfect gas with the SIMPLE algorithm:
//! `∇·(φ U) − ∇·(μ (∇U + ∇Uᵀ − ⅔ (∇·U) I)) = −∇p`, the energy equation,
//! `∇·(ρ U) = 0` and `ρ = ψ p`.
//!
//! The engine's counterpart of OpenFOAM's `rhoSimpleFoam` (OpenFOAM v1912,
//! GPL-3.0-or-later), laminar (`Stokes`), with SIMPLE or SIMPLEC
//! (`consistent`). Its pieces come in the same order: the under-relaxed
//! momentum equation and predictor, the energy equation and the gas
//! properties it gives, `HbyA` and its mass flux, the pressure equation,
//! the corrected flux, the relaxed pressure, the corrected velocity, and
//! the relaxed density. It reads `U`, `p` and `T` (and `phi` and `rho` when
//! present) and writes them back with `phi` and `rho`.

use vortia_core::Vec3;
use vortia_fields::{Data, Location, PatchField, ScalarField, VectorField};
use vortia_foamfile::{Dictionary, Value};
use vortia_fv::correction;
use vortia_fv::limited::{self, GradScheme};
use vortia_fv::ops::{self, Convection, SnGrad};
use vortia_fv::stress::div_dev2_grad_t;
use vortia_fv::vector::{dot_interpolate, solution_directions};
use vortia_fv::{
    Boundary, FvGeometry, LduMatrix, Performance, ScalarBc, ScalarMatrix, VectorBoundary,
    VectorMatrix,
};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::case::{Case, VELOCITY, check_dimensions, key_matches};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::driven::{PRESSURE_KINDS, PressureVelocity, TotalPressure, VELOCITY_KINDS};
use crate::error::{Result, SolverError};
use crate::ico::{Algorithm, Step, constrained_boundary, extrapolated, switch};
use crate::laplacian::Ddt;
use crate::rans::{Model, face_diffusivity};
use crate::simple::{Relaxation, check_options, residual_control};
use crate::thermo::{DENSITY, Flow, Gas, PRESSURE, Thermo};

/// The schemes `rhoSimpleFoam` asks for, checked against what the engine
/// has.
struct Schemes {
    momentum: Convection,
    energy: Convection,
    kinetic: Convection,
    /// `grad(U)`, which the viscous stress and the corrected Laplacian read.
    velocity_grad: GradScheme,
    /// The gradient the energy's corrected Laplacian reads.
    energy_grad: GradScheme,
    momentum_sn_grad: SnGrad,
    energy_sn_grad: SnGrad,
    pressure_sn_grad: SnGrad,
    /// `snGrad(p)`, read only for SIMPLEC.
    correction_sn_grad: SnGrad,
}

impl Schemes {
    fn read(case: &Case, gas: &Gas, consistent: bool) -> Result<Self> {
        if Ddt::read(case, "ddt(rho,U)")? != Ddt::Steady {
            return Err(SolverError::Setup(
                "rhoSimpleFoam is steady: ddt(rho,U) must be steadyState".into(),
            ));
        }
        case.require_linear_interpolation()?;
        let stress = case.scheme("divSchemes", "div(((rho*nuEff)*dev2(T(grad(U)))))")?;
        if stress != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "div scheme '{}' for the viscous stress is not supported yet; use Gauss linear",
                stress.join(" ")
            )));
        }
        case.require_gauss_linear_grad("p")?;
        let e = gas.energy.name();
        let kinetic = match gas.energy {
            crate::thermo::Energy::Internal => "div(phi,Ekp)",
            crate::thermo::Energy::Enthalpy => "div(phi,K)",
        };
        Ok(Self {
            momentum: case.convection_with_grad("div(phi,U)")?,
            energy: case.convection_with_grad(&format!("div(phi,{e})"))?,
            kinetic: case.convection_with_grad(kinetic)?,
            velocity_grad: case.grad_scheme("grad(U)")?,
            energy_grad: case.grad_scheme(&format!("grad({e})"))?,
            momentum_sn_grad: case.laplacian_sn_grad("laplacian((rho*nuEff),U)")?,
            energy_sn_grad: case.laplacian_sn_grad(&format!("laplacian(alphahe,{e})"))?,
            // pEqn.H names the pressure diffusivity rhorAUf, pcEqn.H rhorAtU;
            // SIMPLEC's flux correction is fvc::snGrad(p), a scheme of its own.
            pressure_sn_grad: case.laplacian_sn_grad(if consistent {
                "laplacian(rhorAtU,p)"
            } else {
                "laplacian(rhorAUf,p)"
            })?,
            correction_sn_grad: if consistent {
                case.face_sn_grad("snGrad(p)")?
            } else {
                SnGrad::Uncorrected
            },
        })
    }
}

/// A cell field and its value on every boundary face (`None` on empty
/// faces).
type WithBoundary = (Vec<f64>, Vec<Option<f64>>);

/// The pressure, its conditions, and its value on the boundary faces.
struct Pressure {
    file: ScalarField,
    bc: Boundary,
    /// The `totalPressure` patches, whose values follow the flow.
    total: TotalPressure,
    cells: Vec<f64>,
}

impl Pressure {
    fn read(start: &std::path::Path, mesh: &PolyMesh) -> Result<Self> {
        let file = ScalarField::read(start.join("p"))?;
        check_dimensions("p", &file.dimensions, PRESSURE)?;
        let bc = Boundary::from_field_with(&file, mesh, PRESSURE_KINDS)?;
        let total = TotalPressure::read(&file, mesh)?;
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
        Ok(Self {
            file,
            bc,
            total,
            cells,
        })
    }

    fn boundary(&self, mesh: &PolyMesh, fv: &FvGeometry) -> Vec<Option<f64>> {
        self.bc.face_values(mesh, &self.cells, &fv.delta_coeffs)
    }
}

/// Refuses a turbulence model or source the engine does not have.
fn check_models(case: &Case) -> Result<()> {
    match Model::read(case)? {
        Model::Laminar => {}
        _ => {
            return Err(SolverError::Setup(
                "rhoSimpleFoam with a RAS model is not supported yet; use laminar".into(),
            ));
        }
    }
    check_options(case)?;
    let pressure = case.solution.dict("SIMPLE").cloned().unwrap_or_default();
    if switch(&pressure, "transonic") == Some(true) {
        return Err(SolverError::Setup(
            "SIMPLE transonic is not supported yet".into(),
        ));
    }
    for key in ["pMin", "pMax", "pMinFactor", "pMaxFactor"] {
        if pressure.get(key).is_some() {
            return Err(SolverError::Setup(format!(
                "SIMPLE {key} is not supported yet"
            )));
        }
    }
    Ok(())
}

/// Runs the case and returns what each iteration did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    check_models(case)?;
    let gas = Gas::read(case)?;
    let simple = Algorithm::read(case, "SIMPLE", mesh.n_cells())?;
    let consistent = case
        .solution
        .dict("SIMPLE")
        .and_then(|d| switch(d, "consistent"))
        .unwrap_or(false);
    let schemes = Schemes::read(case, &gas, consistent)?;
    let relaxation = Relaxation::read(case);
    let residuals = residual_control(case);
    let energy = gas.energy.name();
    let (u_solver, u_controls) = LinearSolver::read(case, "U")?;
    let (e_solver, e_controls) = LinearSolver::read(case, energy)?;
    let (p_solver, p_controls) = LinearSolver::read(case, "p")?;
    let run = RunControl::read(case)?;

    let start = case.dir.join(&run.start_name);
    let mut p = Pressure::read(&start, &mesh)?;
    if !p
        .bc
        .conditions
        .iter()
        .any(|c| matches!(c, ScalarBc::FixedValue(_)))
    {
        return Err(SolverError::Setup(
            "rhoSimpleFoam without a fixed pressure anywhere is not supported yet".into(),
        ));
    }
    let mut p_b = p.boundary(&mesh, &fv);
    let mut thermo = Thermo::read(gas, &mesh, &start, (&p.cells, &p_b))?;

    let mut u_field = VectorField::read(start.join("U"))?;
    check_dimensions("U", &u_field.dimensions, VELOCITY)?;
    let mut u_bc = VectorBoundary::from_field_with(&u_field, &mesh, VELOCITY_KINDS)?;
    let u_driven = PressureVelocity::read(&u_field, &mesh, &fv)?;
    let mut u = u_field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("U does not fit the mesh".into()))?;

    let (mut rho, mut rho_b): WithBoundary =
        if start.join("rho").is_file() || start.join("rho.gz").is_file() {
            read_calculated(&mesh, &start.join("rho"), DENSITY)?
        } else {
            (mul(&p.cells, &thermo.psi), mul_b(&p_b, &thermo.psi_b))
        };
    let mut phi = if start.join("phi").is_file() || start.join("phi.gz").is_file() {
        crate::scalar_transport::flux_values(
            &mesh,
            &ScalarField::read(start.join("phi"))?,
            MASS_FLUX,
        )?
    } else {
        let mut u_b = u_bc.face_values(&mesh, &u);
        u_driven.initial_values(&mesh, &mut u_b);
        let rho_u: Vec<Vec3> = rho.iter().zip(&u).map(|(&r, &v)| v * r).collect();
        let rho_u_b: Vec<Option<Vec3>> = rho_b
            .iter()
            .zip(&u_b)
            .map(|(r, v)| r.zip(*v).map(|(r, v)| v * r))
            .collect();
        dot_interpolate(&mesh, &fv, &rho_u, &rho_u_b)
    };

    let valid = solution_directions(&mesh, &fv);
    let names = ["Ux", "Uy", "Uz"];
    let alpha_u = relaxation.equation("U");
    let alpha_e = relaxation.equation(energy);
    let alpha_p = relaxation.field("p");
    let alpha_rho = relaxation.field("rho");
    let mut cumulative = 0.0;

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = run.name(step);
        log(&format!("Time = {name}\n\n"));
        let p_prev = p.cells.clone();
        let p_bc_prev = p.bc.clone();
        let (rho_prev, rho_b_prev) = (rho.clone(), rho_b.clone());
        let mut solves: Vec<(String, Performance)> = Vec::new();

        // UEqn.H: div(phi, U) + divDevRhoReff(U), relaxed, solved with −∇p.
        // Its matrices update the velocity conditions that follow the flow
        // first, from the flux and density of the last iteration.
        u_driven.update(&mesh, &fv, &mut u_bc, &rho_b, &phi);
        let rho_nu: Vec<f64> = rho
            .iter()
            .zip(&thermo.mu)
            .map(|(&r, &m)| r * (m / r))
            .collect();
        let rho_nu_b: Vec<Option<f64>> = rho_b
            .iter()
            .zip(&thermo.mu_b)
            .map(|(r, m)| r.zip(*m).map(|(r, m)| r * (m / r)))
            .collect();
        let rho_nu_f = face_diffusivity(&mesh, &fv, &rho_nu, &rho_nu_b);
        let mut u_eqn =
            VectorMatrix::convection(&mesh, &fv, &u_bc, &phi, &u, schemes.momentum.interpolation)?;
        if schemes.momentum.bounded {
            ops::bounded_correction(&mut u_eqn.ldu, &mesh, &fv, &phi);
        }
        let mut u_lap = VectorMatrix::laplacian(
            &mesh,
            &fv,
            &u_bc,
            &rho_nu_f,
            &u,
            schemes.momentum_sn_grad.without_correction(),
        )?;
        if schemes.momentum_sn_grad == SnGrad::Corrected {
            let (g, _) = vortia_fv::stress::grad(schemes.velocity_grad, &mesh, &fv, &u_bc, &u);
            let corr = correction::sn_grad_correction_vector(&mesh, &fv, &g);
            let flux = correction::scale_by_gamma_mag_sf(&fv, &rho_nu_f, &corr);
            correction::subtract_divergence(&mut u_lap.source, &mesh, &flux);
        }
        u_eqn.sub(&u_lap);
        let stress = div_dev2_grad_t(
            schemes.velocity_grad,
            &mesh,
            &fv,
            &u_bc,
            &u,
            &rho_nu,
            &rho_nu_b,
        );
        for ((s, &d), &v) in u_eqn.source.iter_mut().zip(&stress).zip(fv.volumes()) {
            *s = *s + d * v;
        }
        if let Some(scheme) = schemes.momentum.interpolation.grad_scheme() {
            let (g, _) = vortia_fv::stress::grad(scheme, &mesh, &fv, &u_bc, &u);
            let corr = vortia_fv::stress::linear_upwind_correction(&mesh, &fv, &phi, &g);
            for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
                let (Some(&x), Some(&c)) = (phi.get(f), corr.get(f)) else {
                    continue;
                };
                if let Some(s) = u_eqn.source.get_mut(o) {
                    *s = *s - c * x;
                }
                if let Some(s) = u_eqn.source.get_mut(n) {
                    *s = *s + c * x;
                }
            }
        }
        if let Some(alpha) = alpha_u {
            u_eqn.relax(&mesh, &u, alpha);
        }
        if simple.momentum_predictor {
            let grad_p = ops::grad(&mesh, &fv, &p.bc, &p.cells);
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

        // EEqn.H: div(phi, he) + div(phi, K or Ekp) − laplacian(αEff, he).
        let u_b = u_bc.face_values(&mesh, &u);
        let he_bc = thermo.energy_boundary(
            &mesh,
            &fv,
            &p_b,
            &Flow {
                u_b: &u_b,
                phi: &phi,
            },
        );
        let kinetic = |u: Vec3, p: f64, rho: f64| match gas.energy {
            crate::thermo::Energy::Internal => 0.5 * u.dot(u) + p / rho,
            crate::thermo::Energy::Enthalpy => 0.5 * u.dot(u),
        };
        let k_cells: Vec<f64> = u
            .iter()
            .zip(&p.cells)
            .zip(&rho)
            .map(|((&u, &p), &r)| kinetic(u, p, r))
            .collect();
        let k_b: Vec<Option<f64>> = u_b
            .iter()
            .zip(&p_b)
            .zip(&rho_b)
            .map(|((u, p), r)| match (u, p, r) {
                (Some(u), Some(p), Some(r)) => Some(kinetic(*u, *p, *r)),
                _ => None,
            })
            .collect();
        let div_k = explicit_div(&mesh, &fv, &phi, &k_cells, &k_b, schemes.kinetic);
        let cp_by_cpv = gas.cp_by_cpv();
        let alpha_eff: Vec<f64> = thermo.alpha.iter().map(|&a| cp_by_cpv * a).collect();
        let alpha_eff_b: Vec<Option<f64>> = thermo
            .alpha_b
            .iter()
            .map(|a| a.map(|a| cp_by_cpv * a))
            .collect();
        let alpha_eff_f = face_diffusivity(&mesh, &fv, &alpha_eff, &alpha_eff_b);
        let mut e_eqn =
            ScalarMatrix::convection(&mesh, &fv, &he_bc, &phi, schemes.energy.interpolation)?;
        if schemes.energy.bounded {
            e_eqn.bound_convection(&mesh, &fv, &phi);
        }
        for ((s, &d), &v) in e_eqn.ldu.source.iter_mut().zip(&div_k).zip(fv.volumes()) {
            *s -= v * d;
        }
        if let Some(scheme) = schemes.energy.interpolation.grad_scheme() {
            let g = limited::grad(scheme, &mesh, &fv, &thermo.he, &thermo.he_b);
            let corr = limited::correction_from(&mesh, &fv, &phi, &g);
            for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
                let (Some(&x), Some(&c)) = (phi.get(f), corr.get(f)) else {
                    continue;
                };
                if let Some(s) = e_eqn.ldu.source.get_mut(o) {
                    *s -= c * x;
                }
                if let Some(s) = e_eqn.ldu.source.get_mut(n) {
                    *s += c * x;
                }
            }
        }
        let mut e_lap = ScalarMatrix::laplacian(
            &mesh,
            &fv,
            &he_bc,
            &alpha_eff_f,
            schemes.energy_sn_grad.without_correction(),
        )?;
        if schemes.energy_sn_grad == SnGrad::Corrected {
            let g = limited::grad(schemes.energy_grad, &mesh, &fv, &thermo.he, &thermo.he_b);
            let corr = correction::sn_grad_correction(&mesh, &fv, &g);
            let flux = correction::scale_by_gamma_mag_sf(&fv, &alpha_eff_f, &corr);
            correction::subtract_divergence(&mut e_lap.ldu.source, &mesh, &flux);
        }
        e_eqn.sub(&e_lap);
        if let Some(alpha) = alpha_e {
            e_eqn.relax(&mesh, &thermo.he, alpha);
        }
        let perf = e_eqn.solve(&mesh, &mut thermo.he, |m, x| {
            e_solver.solve(m, x, &e_controls)
        })?;
        log(&solve_line(energy, &perf));
        solves.push((energy.to_owned(), perf));
        thermo.he_b = he_bc.face_values(&mesh, &thermo.he, &fv.delta_coeffs);
        thermo.correct(&mesh, &p.cells, &p_b)?;

        // pEqn.H or pcEqn.H; the latter first takes the density of the
        // gas just corrected.
        if consistent {
            rho = mul(&p.cells, &thermo.psi);
            rho_b = mul_b(&p_b, &thermo.psi_b);
        }
        let a = u_eqn.a(&mesh, &fv);
        let r_au: Vec<f64> = a.iter().map(|&a| 1.0 / a).collect();
        let r_atu: Vec<f64> = if consistent {
            let h1 = u_eqn.h1(&fv);
            r_au.iter()
                .zip(&h1)
                .map(|(&r, &h)| 1.0 / (1.0 / r - h))
                .collect()
        } else {
            r_au.clone()
        };
        let h = u_eqn.h(&mesh, &fv, &u, valid);
        let mut hbya: Vec<Vec3> = h.iter().zip(&r_au).map(|(&h, &r)| h * r).collect();
        let hbya_b = constrained_boundary(&mesh, &u_bc, &u, &hbya);
        let rho_f = ops::interpolate_linear(&mesh, &fv, &rho, &rho_b);
        let flux = dot_interpolate(&mesh, &fv, &hbya, &hbya_b);
        let mut phi_hbya: Vec<f64> = rho_f.iter().zip(&flux).map(|(&r, &f)| r * f).collect();
        let r_atu_b = extrapolated(&mesh, &r_atu);
        let rho_r_atu: Vec<f64> = mul(&rho, &r_atu);
        let rho_r_atu_b = mul_b(&rho_b, &r_atu_b);
        if consistent {
            let diff: Vec<f64> = r_atu.iter().zip(&r_au).map(|(&t, &a)| t - a).collect();
            let diff_b = extrapolated(&mesh, &diff);
            let rho_diff = mul(&rho, &diff);
            let rho_diff_b = mul_b(&rho_b, &diff_b);
            let rho_diff_f = ops::interpolate_linear(&mesh, &fv, &rho_diff, &rho_diff_b);
            let sn = sn_grad(&mesh, &fv, &p.bc, &p.cells, schemes.correction_sn_grad);
            for (f, x) in phi_hbya.iter_mut().enumerate() {
                let (Some(&r), Some(&g), Some(&a)) =
                    (rho_diff_f.get(f), sn.get(f), fv.mag_sf.get(f))
                else {
                    continue;
                };
                *x += (r * g) * a;
            }
            let grad_p = ops::grad(&mesh, &fv, &p.bc, &p.cells);
            for (((hb, &ra), &rt), &g) in hbya.iter_mut().zip(&r_au).zip(&r_atu).zip(&grad_p) {
                *hb = *hb - g * (ra - rt);
            }
        }
        let rho_r_atu_f = ops::interpolate_linear(&mesh, &fv, &rho_r_atu, &rho_r_atu_b);
        // The pressure matrix updates `totalPressure` from the flux of the
        // last iteration and the velocity just solved.
        p.total.update(&mesh, &mut p.bc, &rho_b, &u_b, &phi);
        for non_orth in 0..=simple.n_non_orth {
            let mut m = LduMatrix::new(&mesh);
            ops::laplacian(
                &mut m,
                &mesh,
                &fv,
                &p.bc,
                &rho_r_atu_f,
                schemes.pressure_sn_grad.without_correction(),
            )?;
            // The correction is explicit, from the pressure of the matrix's
            // assembly; its flux goes into the face flux as well.
            let corrected = (schemes.pressure_sn_grad == SnGrad::Corrected).then(|| {
                let g = ops::grad(&mesh, &fv, &p.bc, &p.cells);
                let corr = correction::sn_grad_correction(&mesh, &fv, &g);
                correction::scale_by_gamma_mag_sf(&fv, &rho_r_atu_f, &corr)
            });
            if let Some(flux) = &corrected {
                correction::subtract_divergence(&mut m.source, &mesh, flux);
            }
            let div = ops::surface_integrate(&mesh, &fv, &phi_hbya);
            for ((s, &d), &v) in m.source.iter_mut().zip(&div).zip(fv.volumes()) {
                *s += v * d;
            }
            let perf = p_solver.solve(&m, &mut p.cells, &p_controls)?;
            log(&solve_line("p", &perf));
            solves.push(("p".to_owned(), perf));
            if non_orth == simple.n_non_orth {
                let mut flux = ops::laplacian_flux(
                    &mesh,
                    &fv,
                    &p.bc,
                    &rho_r_atu_f,
                    schemes.pressure_sn_grad.without_correction(),
                    &p.cells,
                );
                for (x, c) in flux.iter_mut().zip(corrected.iter().flatten()) {
                    *x += c;
                }
                for ((f, &a), &b) in phi.iter_mut().zip(&phi_hbya).zip(&flux) {
                    *f = a - b;
                }
            }
        }
        cumulative += continuity_errors(&mesh, &fv, &phi, run.delta_t, cumulative, &mut log);

        if let Some(alpha) = alpha_p {
            for (pc, &prev) in p.cells.iter_mut().zip(&p_prev) {
                *pc = prev + alpha * (*pc - prev);
            }
            p.total.relax(&mut p.bc, &p_bc_prev, alpha);
        }
        p_b = p.boundary(&mesh, &fv);
        let grad_p = ops::grad(&mesh, &fv, &p.bc, &p.cells);
        for (((uc, &hb), &r), &g) in u.iter_mut().zip(&hbya).zip(&r_atu).zip(&grad_p) {
            *uc = hb - g * r;
        }
        // `U.correctBoundaryConditions()`: the conditions that follow the flow
        // take the new flux and the density of this iteration.
        u_driven.update(&mesh, &fv, &mut u_bc, &rho_b, &phi);
        rho = mul(&p.cells, &thermo.psi);
        rho_b = mul_b(&p_b, &thermo.psi_b);
        if let Some(alpha) = alpha_rho {
            for (r, &prev) in rho.iter_mut().zip(&rho_prev) {
                *r = prev + alpha * (*r - prev);
            }
            for (r, prev) in rho_b.iter_mut().zip(&rho_b_prev) {
                if let (Some(r), Some(prev)) = (r.as_mut(), prev) {
                    *r = prev + alpha * (*r - prev);
                }
            }
        }

        let converged = !residuals.is_empty() && converged(&residuals, &solves);
        let written = run.writes(step) || converged;
        if written {
            u_field.internal = Data::NonUniform(u.clone());
            u_field.drop_zero_gradient_values();
            u_field.store_patch_values(&mesh, VELOCITY_KINDS, &u_bc.face_values(&mesh, &u));
            u_field.write(&case.dir, &name)?;
            p.file.internal = Data::NonUniform(p.cells.clone());
            p.file.drop_zero_gradient_values();
            p.file.store_patch_values(&mesh, PRESSURE_KINDS, &p_b);
            p.file.write(&case.dir, &name)?;
            thermo.to_write(&mesh).write(&case.dir, &name)?;
            calculated_field(&mesh, "rho", Location::Cells, DENSITY, &rho, &rho_b)
                .write(&case.dir, &name)?;
            let (phi_cells, phi_b) = split_faces(&mesh, &phi);
            calculated_field(&mesh, "phi", Location::Faces, MASS_FLUX, &phi_cells, &phi_b)
                .write(&case.dir, &name)?;
        }
        steps.push(Step {
            time: name,
            solves,
            written,
        });
        if converged {
            log(&format!(
                "\nSIMPLE solution converged in {step} iterations\n\n"
            ));
            break;
        }
    }
    log("End\n");
    Ok(steps)
}

const MASS_FLUX: [i32; 7] = [1, 0, -1, 0, 0, 0, 0];

fn mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(&a, &b)| a * b).collect()
}

fn mul_b(a: &[Option<f64>], b: &[Option<f64>]) -> Vec<Option<f64>> {
    a.iter()
        .zip(b)
        .map(|(a, b)| a.zip(*b).map(|(a, b)| a * b))
        .collect()
}

/// `fvc::div(φ, ψ)` with `Gauss upwind`, `linear` or `linearUpwind`, and its
/// `bounded` variant, which takes `(∇·φ) ψ` back out.
fn explicit_div(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    phi: &[f64],
    cells: &[f64],
    boundary: &[Option<f64>],
    scheme: Convection,
) -> Vec<f64> {
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or(0.0);
    let corr = scheme.interpolation.grad_scheme().map(|gs| {
        let g = limited::grad(gs, mesh, fv, cells, boundary);
        limited::correction_from(mesh, fv, phi, &g)
    });
    let mut flux = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        let x = phi.get(f).copied().unwrap_or(0.0);
        let w = scheme.interpolation.weight(fv, f, x);
        let c = corr.as_ref().and_then(|c| c.get(f)).copied().unwrap_or(0.0);
        flux.push(x * ((w * (o - n) + n) + c));
    }
    for (k, b) in boundary.iter().enumerate() {
        let f = mesh.n_internal_faces() + k;
        flux.push(phi.get(f).copied().unwrap_or(0.0) * b.unwrap_or(0.0));
    }
    let mut div = ops::surface_integrate(mesh, fv, &flux);
    if scheme.bounded {
        let net = ops::surface_integrate(mesh, fv, phi);
        for ((d, &n), &c) in div.iter_mut().zip(&net).zip(cells) {
            *d -= n * c;
        }
    }
    div
}

/// `fvc::snGrad(ψ)` on every face: across the internal faces, and from the
/// conditions on the boundary (none through a zero gradient).
fn sn_grad(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    cells: &[f64],
    scheme: SnGrad,
) -> Vec<f64> {
    let coeffs = scheme.coeffs(fv);
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or(0.0);
    // `correctedSnGrad`: the interpolated cell gradient along the
    // non-orthogonal part of the face vector, added to the difference.
    let corr = (scheme == SnGrad::Corrected).then(|| {
        let g = ops::grad(mesh, fv, bc, cells);
        correction::sn_grad_correction(mesh, fv, &g)
    });
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        let c = corr.as_ref().and_then(|c| c.get(f)).copied();
        out.push(coeffs.get(f).copied().unwrap_or(0.0) * (n - o) + c.unwrap_or(0.0));
    }
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let inside = cell(mesh.owner().get(f));
            let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
            out.push(match cond {
                ScalarBc::FixedValue(v) | ScalarBc::Calculated(v) => {
                    dc * (v.get(i).copied().unwrap_or(0.0) - inside)
                }
                ScalarBc::FixedGradient(g) => g.get(i).copied().unwrap_or(0.0),
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let f_m = fraction.get(i).copied().unwrap_or(0.0);
                    f_m * dc * (ref_value.get(i).copied().unwrap_or(0.0) - inside)
                }
                ScalarBc::ZeroGradient | ScalarBc::Reflect | ScalarBc::Empty => 0.0,
            });
        }
    }
    out
}

/// `incompressible/continuityErrs.H`: logs the volume-weighted mean of
/// `|∇·φ|` and of `∇·φ` times the step, and returns the second.
fn continuity_errors(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    phi: &[f64],
    dt: f64,
    cumulative: f64,
    log: &mut dyn FnMut(&str),
) -> f64 {
    let div = ops::surface_integrate(mesh, fv, phi);
    let total: f64 = fv.volumes().iter().sum();
    let (mut local, mut global) = (0.0, 0.0);
    for (&d, &v) in div.iter().zip(fv.volumes()) {
        local += v * d.abs();
        global += v * d;
    }
    let (local, global) = (dt * (local / total), dt * (global / total));
    log(&format!(
        "time step continuity errors : sum local = {local}, global = {global}, cumulative = {}\n",
        cumulative + global
    ));
    global
}

/// Converged when every solved field that `residualControl` names began
/// this iteration below its tolerance; a vector by its largest component.
fn converged(control: &[(String, f64)], solves: &[(String, Performance)]) -> bool {
    let mut checked = false;
    let mut seen: Vec<&str> = Vec::new();
    for (name, _) in solves {
        let field = match name.as_str() {
            "Ux" | "Uy" | "Uz" => "U",
            other => other,
        };
        if seen.contains(&field) {
            continue;
        }
        seen.push(field);
        let Some(tol) = control
            .iter()
            .find(|(k, _)| key_matches(k, field))
            .map(|&(_, t)| t)
        else {
            continue;
        };
        let first = solves
            .iter()
            .filter(|(n, _)| {
                n == field || (field == "U" && matches!(n.as_str(), "Ux" | "Uy" | "Uz"))
            })
            .map(|(_, p)| p.initial_residual)
            .fold(f64::NEG_INFINITY, f64::max);
        if first >= tol {
            return false;
        }
        checked = true;
    }
    checked
}

/// A field's internal values and boundary values, faces in patch order;
/// only what the solver writes back on a restart.
fn read_calculated(
    mesh: &PolyMesh,
    path: &std::path::Path,
    dims: [i32; 7],
) -> Result<WithBoundary> {
    let file = ScalarField::read(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    check_dimensions(&name, &file.dimensions, dims)?;
    file.check_against(mesh)?;
    let cells = file
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup(format!("{name} does not fit the mesh")))?;
    let mut boundary = Vec::new();
    for patch in mesh.patches() {
        let values = file
            .patch_field(patch)
            .and_then(|pf| pf.value.as_ref())
            .and_then(|d| d.expand(patch.size));
        for (i, f) in patch.faces().enumerate() {
            boundary.push(if patch.kind == PatchKind::Empty {
                None
            } else {
                values
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .or_else(|| mesh.owner().get(f).and_then(|&c| cells.get(c)).copied())
            });
        }
    }
    Ok((cells, boundary))
}

/// A face field's internal faces and its boundary faces (`None` on empty
/// patches).
fn split_faces(mesh: &PolyMesh, phi: &[f64]) -> WithBoundary {
    let n = mesh.n_internal_faces();
    let internal = phi.iter().take(n).copied().collect();
    let mut boundary = Vec::new();
    for patch in mesh.patches() {
        for f in patch.faces() {
            boundary
                .push((patch.kind != PatchKind::Empty).then(|| phi.get(f).copied().unwrap_or(0.0)));
        }
    }
    (internal, boundary)
}

/// A field with `calculated` conditions, as OpenFOAM writes `rho` and `phi`.
fn calculated_field(
    mesh: &PolyMesh,
    name: &str,
    location: Location,
    dims: [i32; 7],
    internal: &[f64],
    boundary: &[Option<f64>],
) -> ScalarField {
    let n_internal = mesh.n_internal_faces();
    let patches = mesh
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
                            .map(|f| {
                                boundary
                                    .get(f - n_internal)
                                    .copied()
                                    .flatten()
                                    .unwrap_or(0.0)
                            })
                            .collect(),
                    )
                }),
                settings: Dictionary::new(),
            }
        })
        .collect();
    ScalarField {
        name: name.into(),
        location,
        dimensions: Value::Dimensions(
            dims.into_iter()
                .map(|d| Value::Label(i64::from(d)))
                .collect(),
        ),
        internal: Data::NonUniform(internal.to_vec()),
        boundary: patches,
        boundary_directives: Dictionary::new(),
        extra: Dictionary::new(),
    }
}
