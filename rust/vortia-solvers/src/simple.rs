//! Steady incompressible laminar flow with the SIMPLE algorithm:
//! `∇·(φ U) − ∇·(ν (∇U + ∇Uᵀ)) = −∇p`, `∇·U = 0`.
//!
//! The engine's counterpart of OpenFOAM's `simpleFoam` (OpenFOAM,
//! GPL-3.0-or-later) with the laminar `Stokes` model, its pieces in the same
//! order: the under-relaxed momentum equation and predictor, `HbyA` and its
//! flux, the pressure equation, the corrected flux, the relaxed pressure and
//! the corrected velocity. Each iteration is one "time" step; the run stops
//! early when every field in `SIMPLE/residualControl` starts its iteration
//! below its tolerance. It reads `U`, `p` (and `phi` when present) and `nu`,
//! and writes `U`, `p` and `phi`.

use vortia_core::Vec3;
use vortia_fields::{Data, ScalarField, VectorField};
use vortia_foamfile::Dictionary;
use vortia_fv::ops::{self, Convection, SnGrad};
use vortia_fv::stress::div_dev2_grad_t;
use vortia_fv::vector::{dot_interpolate, solution_directions};
use vortia_fv::{Boundary, LduMatrix, Performance, ScalarBc, VectorBoundary, VectorMatrix};

use crate::case::{
    Case, KINEMATIC_DIFFUSIVITY, KINEMATIC_PRESSURE, VELOCITY, check_dimensions, key_matches,
    read_dict,
};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::ico::{
    Algorithm, Step, adjust_phi, as_str, constrained_boundary, extrapolated, flux_field, switch,
};
use crate::laplacian::{Ddt, transport_property};
use crate::scalar_transport::face_values;

/// The schemes `simpleFoam` asks for, checked against what the engine has.
struct Schemes {
    convection: Convection,
    momentum_sn_grad: SnGrad,
    pressure_sn_grad: SnGrad,
}

impl Schemes {
    fn read(case: &Case) -> Result<Self> {
        if Ddt::read(case, "ddt(U)")? != Ddt::Steady {
            return Err(SolverError::Setup(
                "simpleFoam is steady: ddt(U) must be steadyState".into(),
            ));
        }
        let words = |section: &str, key: &str| case.scheme(section, key);
        let convection = Convection::from_scheme(&as_str(&words("divSchemes", "div(phi,U)")?))?;
        let stress = words("divSchemes", "div((nuEff*dev2(T(grad(U)))))")?;
        if stress != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "div scheme '{}' for the viscous stress is not supported yet; use Gauss linear",
                stress.join(" ")
            )));
        }
        let sn = |key: &str| -> Result<SnGrad> {
            let w = words("laplacianSchemes", key)?;
            let s = SnGrad::from_scheme(&as_str(&w))?;
            if s == SnGrad::Corrected {
                return Err(SolverError::Setup(format!(
                    "laplacian scheme '{}' for {key} is not supported yet by simpleFoam; \
                     use Gauss linear orthogonal or uncorrected",
                    w.join(" ")
                )));
            }
            Ok(s)
        };
        for field in ["U", "p"] {
            let w = words("gradSchemes", &format!("grad({field})"))?;
            if w != ["Gauss", "linear"] {
                return Err(SolverError::Setup(format!(
                    "grad scheme '{}' is not supported yet; use Gauss linear",
                    w.join(" ")
                )));
            }
        }
        Ok(Self {
            convection,
            momentum_sn_grad: sn("laplacian(nuEff,U)")?,
            pressure_sn_grad: sn("laplacian((1|A(U)),p)")?,
        })
    }
}

/// `fvSolution/relaxationFactors`: `fields` and `equations`, or the older
/// flat form, where names starting with `p` are fields and every name an
/// equation.
struct Relaxation {
    fields: Dictionary,
    equations: Dictionary,
}

impl Relaxation {
    fn read(case: &Case) -> Self {
        let d = case
            .solution
            .dict("relaxationFactors")
            .cloned()
            .unwrap_or_default();
        if d.dict("fields").is_some() || d.dict("equations").is_some() {
            return Self {
                fields: d.dict("fields").cloned().unwrap_or_default(),
                equations: d.dict("equations").cloned().unwrap_or_default(),
            };
        }
        let mut fields = Dictionary::new();
        for (k, e) in d.entries() {
            if k.starts_with('p') {
                fields.insert(k, e.clone());
            }
        }
        Self {
            fields,
            equations: d,
        }
    }

    fn factor(d: &Dictionary, name: &str) -> Option<f64> {
        d.scalar(name)
            .or_else(|| {
                d.entries().rev().find_map(|(k, _)| {
                    (k.starts_with('"') || k.contains('|') || k.contains(".*"))
                        .then(|| key_matches(k.trim_matches('"'), name))
                        .filter(|&m| m)
                        .and_then(|_| d.scalar(k))
                })
            })
            .or_else(|| d.scalar("default"))
            .filter(|f| f.is_finite())
    }

    fn field(&self, name: &str) -> Option<f64> {
        Self::factor(&self.fields, name)
    }

    fn equation(&self, name: &str) -> Option<f64> {
        Self::factor(&self.equations, name)
    }
}

/// `SIMPLE/residualControl`: the tolerance under which a field's first
/// initial residual of an iteration counts as converged.
fn residual_control(case: &Case) -> Vec<(String, f64)> {
    let Some(d) = case
        .solution
        .dict("SIMPLE")
        .and_then(|s| s.dict("residualControl"))
    else {
        return Vec::new();
    };
    d.entries()
        .filter_map(|(k, _)| d.scalar(k).map(|t| (k.trim_matches('"').to_owned(), t)))
        .collect()
}

/// Refuses a turbulence or transport model other than the one the engine
/// has: laminar flow of a Newtonian fluid.
fn check_models(case: &Case) -> Result<()> {
    let constant = case.dir.join("constant");
    let turbulence = constant.join("turbulenceProperties");
    if turbulence.is_file() {
        let d = read_dict(&turbulence)?;
        match d.word("simulationType") {
            Some("laminar") => {}
            other => {
                return Err(SolverError::Setup(format!(
                    "simulationType '{}' is not supported yet; use laminar",
                    other.unwrap_or("")
                )));
            }
        }
        if let Some(model) = d.dict("laminar").and_then(|l| l.word("model"))
            && model != "Stokes"
        {
            return Err(SolverError::Setup(format!(
                "laminar model '{model}' is not supported yet; use Stokes"
            )));
        }
    }
    let transport = read_dict(&constant.join("transportProperties"))?;
    match transport.word("transportModel") {
        None | Some("Newtonian") => Ok(()),
        Some(other) => Err(SolverError::Setup(format!(
            "transportModel '{other}' is not supported yet; use Newtonian"
        ))),
    }
}

/// Runs the case and returns what each iteration did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    check_models(case)?;
    let nu = transport_property(case, "nu", KINEMATIC_DIFFUSIVITY)?;
    let schemes = Schemes::read(case)?;
    let simple = Algorithm::read(case, "SIMPLE", mesh.n_cells())?;
    if case
        .solution
        .dict("SIMPLE")
        .and_then(|d| switch(d, "consistent"))
        .unwrap_or(false)
    {
        return Err(SolverError::Setup(
            "SIMPLE consistent (SIMPLEC) is not supported yet".into(),
        ));
    }
    let relaxation = Relaxation::read(case);
    let residuals = residual_control(case);
    let (u_solver, u_controls) = LinearSolver::read(case, "U")?;
    let (p_solver, p_controls) = LinearSolver::read(case, "p")?;
    let run = RunControl::read(case)?;

    let start = case.dir.join(&run.start_name);
    let mut u_field = VectorField::read(start.join("U"))?;
    let mut p_field = ScalarField::read(start.join("p"))?;
    check_dimensions("U", &u_field.dimensions, VELOCITY)?;
    check_dimensions("p", &p_field.dimensions, KINEMATIC_PRESSURE)?;
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
    let alpha_u = relaxation.equation("U");
    let alpha_p = relaxation.field("p");

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        let name = run.name(step);
        log(&format!("Time = {name}\n\n"));
        let p_prev = p.clone();
        let mut solves: Vec<(String, Performance)> = Vec::new();

        let mut u_eqn =
            VectorMatrix::convection(&mesh, &fv, &u_bc, &phi, schemes.convection.interpolation)?;
        if schemes.convection.bounded {
            ops::bounded_correction(&mut u_eqn.ldu, &mesh, &fv, &phi);
        }
        u_eqn.sub(&VectorMatrix::laplacian(
            &mesh,
            &fv,
            &u_bc,
            nu,
            schemes.momentum_sn_grad,
        )?);
        let stress = div_dev2_grad_t(&mesh, &fv, &u_bc, &u, nu);
        for ((s, &d), &v) in u_eqn.source.iter_mut().zip(&stress).zip(fv.volumes()) {
            *s = *s + d * v;
        }
        if let Some(alpha) = alpha_u {
            u_eqn.relax(&mesh, &u, alpha);
        }

        if simple.momentum_predictor {
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

        let r_au: Vec<f64> = u_eqn.a(&mesh, &fv).iter().map(|&a| 1.0 / a).collect();
        let h = u_eqn.h(&mesh, &fv, &u, valid);
        let hbya: Vec<Vec3> = h.iter().zip(&r_au).map(|(&h, &r)| h * r).collect();
        let hbya_b = constrained_boundary(&mesh, &u_bc, &u, &hbya);
        let r_au_f = ops::interpolate_linear(&mesh, &fv, &r_au, &extrapolated(&mesh, &r_au));
        let mut phi_hbya = dot_interpolate(&mesh, &fv, &hbya, &hbya_b);
        if need_reference {
            adjust_phi(&mesh, &u_bc, &mut phi_hbya)?;
        }

        for non_orth in 0..=simple.n_non_orth {
            let mut m = LduMatrix::new(&mesh);
            ops::laplacian(&mut m, &mesh, &fv, &p_bc, &r_au_f, schemes.pressure_sn_grad)?;
            let div = ops::surface_integrate(&mesh, &fv, &phi_hbya);
            for ((s, &d), &v) in m.source.iter_mut().zip(&div).zip(fv.volumes()) {
                *s += v * d;
            }
            if need_reference {
                ops::set_reference(&mut m, simple.ref_cell, simple.ref_value);
            }
            let perf = p_solver.solve(&m, &mut p, &p_controls)?;
            log(&solve_line("p", &perf));
            solves.push(("p".to_owned(), perf));
            if non_orth == simple.n_non_orth {
                let flux =
                    ops::laplacian_flux(&mesh, &fv, &p_bc, &r_au_f, schemes.pressure_sn_grad, &p);
                for ((f, &a), &b) in phi.iter_mut().zip(&phi_hbya).zip(&flux) {
                    *f = a - b;
                }
            }
        }

        if let Some(alpha) = alpha_p {
            for (pc, &prev) in p.iter_mut().zip(&p_prev) {
                *pc = prev + alpha * (*pc - prev);
            }
        }
        let grad_p = ops::grad(&mesh, &fv, &p_bc, &p);
        for (((uc, &hb), &r), &g) in u.iter_mut().zip(&hbya).zip(&r_au).zip(&grad_p) {
            *uc = hb - g * r;
        }

        let converged = !residuals.is_empty() && converged(&residuals, &solves);
        let written = run.writes(step) || converged;
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

/// Whether every field `residualControl` names began this iteration below
/// its tolerance; a vector by its largest component.
fn converged(control: &[(String, f64)], solves: &[(String, Performance)]) -> bool {
    let first = |field: &str| -> Option<f64> {
        let of = |name: &str| {
            solves
                .iter()
                .find(|(f, _)| f == name)
                .map(|(_, p)| p.initial_residual)
        };
        if field == "U" {
            ["Ux", "Uy", "Uz"]
                .iter()
                .filter_map(|c| of(c))
                .reduce(f64::max)
        } else {
            of(field)
        }
    };
    let mut checked = false;
    for field in ["U", "p"] {
        let Some(tol) = control
            .iter()
            .rev()
            .find(|(k, _)| key_matches(k, field))
            .map(|&(_, t)| t)
        else {
            continue;
        };
        match first(field) {
            Some(r) if r < tol => checked = true,
            Some(_) => return false,
            None => {}
        }
    }
    checked
}
