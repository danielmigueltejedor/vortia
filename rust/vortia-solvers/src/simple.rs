//! Steady incompressible flow with the SIMPLE algorithm:
//! `∇·(φ U) − ∇·(ν_eff (∇U + ∇Uᵀ)) = −∇p`, `∇·U = 0`.
//!
//! The engine's counterpart of OpenFOAM's `simpleFoam` (OpenFOAM,
//! GPL-3.0-or-later), laminar (`Stokes`) or with the k-ε or k-ω SST
//! model, `ν_eff = ν + ν_t`. Its pieces come in the same
//! order: the under-relaxed momentum equation and predictor, `HbyA` and its
//! flux, the pressure equation, the corrected flux, the relaxed pressure, the
//! corrected velocity and then the turbulence model. Each iteration is one
//! "time" step; the run stops early when every field in
//! `SIMPLE/residualControl` starts its iteration below its tolerance. It reads `U`, `p` (and `phi` when present) and `nu`,
//! with a turbulence model also `k`, `nut` and `epsilon` or `omega`, and
//! writes them back with `phi`.

use vortia_core::Vec3;
use vortia_foamfile::{Dictionary, Entry};
use vortia_fv::correction;
use vortia_fv::limited::GradScheme;
use vortia_fv::ops::{self, Convection, SnGrad};
use vortia_fv::stress::div_dev2_grad_t;
use vortia_fv::vector::{VectorBoundary, dot_interpolate, solution_directions};
use vortia_fv::{FvGeometry, LduMatrix, Performance, VectorMatrix};
use vortia_mesh::PolyMesh;

use crate::case::{Case, KINEMATIC_DIFFUSIVITY, key_matches, read_dict};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::explicit;
use crate::ico::{
    Algorithm, FlowFields, Step, adjust_phi, constrained_boundary, extrapolated, flux_field,
    switch, write_flow,
};
use crate::laplacian::{Ddt, transport_property};
use crate::rans::{Model, Turbulence};

/// The schemes `simpleFoam` asks for, checked against what the engine has.
pub(crate) struct Schemes {
    pub(crate) convection: Convection,
    pub(crate) momentum_sn_grad: SnGrad,
    pub(crate) pressure_sn_grad: SnGrad,
}

impl Schemes {
    /// The schemes of a steady solver whose pressure is the field `p`
    /// (`p_rgh` for the buoyant ones).
    pub(crate) fn read(case: &Case, p: &str) -> Result<Self> {
        if Ddt::read(case, "ddt(U)")? != Ddt::Steady {
            return Err(SolverError::Setup(
                "a steady solver needs ddt(U) steadyState".into(),
            ));
        }
        case.require_linear_interpolation()?;
        let convection = case.convection_with_grad("div(phi,U)")?;
        let stress = case.scheme("divSchemes", "div((nuEff*dev2(T(grad(U)))))")?;
        if stress != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "div scheme '{}' for the viscous stress is not supported yet; use Gauss linear",
                stress.join(" ")
            )));
        }
        for field in ["U", p] {
            case.require_gauss_linear_grad(field)?;
        }
        Ok(Self {
            convection,
            momentum_sn_grad: case.laplacian_sn_grad("laplacian(nuEff,U)")?,
            pressure_sn_grad: case.laplacian_sn_grad(&format!("laplacian((1|A(U)),{p})"))?,
        })
    }
}

/// `fvSolution/relaxationFactors`: `fields` and `equations`, or the older
/// flat form, where names starting with `p` are fields and every name an
/// equation.
pub(crate) struct Relaxation {
    fields: Dictionary,
    equations: Dictionary,
}

impl Relaxation {
    pub(crate) fn read(case: &Case) -> Self {
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

    pub(crate) fn field(&self, name: &str) -> Option<f64> {
        Self::factor(&self.fields, name)
    }

    pub(crate) fn equation(&self, name: &str) -> Option<f64> {
        Self::factor(&self.equations, name)
    }
}

/// `SIMPLE/residualControl`: the tolerance under which a field's first
/// initial residual of an iteration counts as converged.
pub(crate) fn residual_control(case: &Case) -> Vec<(String, f64)> {
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

/// Refuses a turbulence or transport model the engine does not have: it
/// has laminar flow, k-ε and k-ω SST, of a Newtonian fluid.
pub(crate) fn check_models(case: &Case) -> Result<()> {
    Model::read(case)?;
    check_options(case)?;
    let transport = read_dict(&case.dir.join("constant").join("transportProperties"))?;
    match transport.word("transportModel") {
        None | Some("Newtonian") => Ok(()),
        Some(other) => Err(SolverError::Setup(format!(
            "transportModel '{other}' is not supported yet; use Newtonian"
        ))),
    }
}

/// Refuses active `MRFProperties` and `fvOptions`, which the engine does
/// not have.
pub(crate) fn check_options(case: &Case) -> Result<()> {
    for (dir, name) in [
        ("constant", "MRFProperties"),
        ("constant", "fvOptions"),
        ("system", "fvOptions"),
    ] {
        let path = case.dir.join(dir).join(name);
        if (path.is_file() || path.with_extension("gz").is_file())
            && has_active_entry(&read_dict(&path)?)
        {
            return Err(SolverError::Setup(format!(
                "{dir}/{name} is not supported yet; remove it or set its entries inactive"
            )));
        }
    }
    Ok(())
}

/// Whether `d` holds a sub-dictionary that is not `active false`, which is
/// how `MRFProperties` and `fvOptions` switch their entries on and off.
fn has_active_entry(d: &Dictionary) -> bool {
    d.entries().any(|(_, e)| match e {
        Entry::Dict(sub) => switch(sub, "active") != Some(false),
        Entry::Directive(_) => true,
        Entry::Values(_) => false,
    })
}

/// The viscosity of the momentum equation: in the cells, on the boundary
/// faces, and on every face.
pub(crate) struct Viscosity<'a> {
    pub(crate) cells: &'a [f64],
    pub(crate) boundary: &'a [Option<f64>],
    pub(crate) faces: &'a [f64],
}

/// The momentum equation of a steady solver without its pressure term:
/// convection, `ν_eff` diffusion and the viscous stress's transpose part
/// (OpenFOAM's `fvm::div(phi, U) + divDevReff(U)`), under-relaxed by
/// `alpha_u`.
pub(crate) fn momentum_equation(
    (mesh, fv): (&PolyMesh, &FvGeometry),
    (u_bc, u, phi): (&VectorBoundary, &[Vec3], &[f64]),
    nu: &Viscosity,
    schemes: &Schemes,
    alpha_u: Option<f64>,
) -> Result<VectorMatrix> {
    let mut u_eqn =
        VectorMatrix::convection(mesh, fv, u_bc, phi, u, schemes.convection.interpolation)?;
    if schemes.convection.bounded {
        ops::bounded_correction(&mut u_eqn.ldu, mesh, fv, phi);
    }
    let mut u_lap = VectorMatrix::laplacian(
        mesh,
        fv,
        u_bc,
        nu.faces,
        u,
        schemes.momentum_sn_grad.without_correction(),
    )?;
    // `Gauss linear corrected`: the non-orthogonal part is explicit, from
    // the velocity gradient of this iteration.
    let velocity = explicit::Velocity { bc: u_bc, cells: u };
    if schemes.momentum_sn_grad == SnGrad::Corrected {
        explicit::subtract_vector_laplacian_correction(
            &mut u_lap.source,
            (mesh, fv),
            GradScheme::GaussLinear,
            &velocity,
            nu.faces,
        );
    }
    u_eqn.sub(&u_lap);
    let stress = div_dev2_grad_t(
        GradScheme::GaussLinear,
        mesh,
        fv,
        u_bc,
        u,
        nu.cells,
        nu.boundary,
    );
    for ((s, &d), &v) in u_eqn.source.iter_mut().zip(&stress).zip(fv.volumes()) {
        *s = *s + d * v;
    }
    // `linearUpwind`: the matrix holds the upwind part; the gradient's
    // correction of each face value goes to the source, as `fvm::div`
    // adds it.
    if let Some(scheme) = schemes.convection.interpolation.grad_scheme() {
        explicit::subtract_linear_upwind_correction(
            &mut u_eqn.source,
            (mesh, fv),
            scheme,
            &velocity,
            phi,
        );
    }
    if let Some(alpha) = alpha_u {
        u_eqn.relax(mesh, u, alpha);
    }
    Ok(u_eqn)
}

/// Runs the case and returns what each iteration did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    check_models(case)?;
    let nu = transport_property(case, "nu", KINEMATIC_DIFFUSIVITY)?;
    let schemes = Schemes::read(case, "p")?;
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
    let FlowFields {
        mut u_field,
        mut p_field,
        u_bc,
        p_bc,
        mut u,
        mut p,
        mut phi,
        need_reference,
    } = FlowFields::read(&start, &mesh, &fv)?;
    let mut turbulence =
        Turbulence::read(case, &mesh, &fv, &start, nu, (&u_bc, &u, &[]), |name| {
            relaxation.equation(name)
        })?;
    let valid = solution_directions(&mesh, &fv);
    let names = ["Ux", "Uy", "Uz"];
    let alpha_u = relaxation.equation("U");
    let alpha_p = relaxation.field("p");

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        crate::cancel::check()?;
        let name = run.name(step);
        log(&format!("Time = {name}\n\n"));
        let p_prev = p.clone();
        let mut solves: Vec<(String, Performance)> = Vec::new();

        let (nu_cells, nu_boundary, nu_faces) = match &turbulence {
            Some(t) => t.nu_eff(&mesh, &fv),
            None => (
                vec![nu; mesh.n_cells()],
                vec![Some(nu); mesh.n_faces().saturating_sub(mesh.n_internal_faces())],
                vec![nu; mesh.n_faces()],
            ),
        };
        let u_eqn = momentum_equation(
            (&mesh, &fv),
            (&u_bc, &u, &phi),
            &Viscosity {
                cells: &nu_cells,
                boundary: &nu_boundary,
                faces: &nu_faces,
            },
            &schemes,
            alpha_u,
        )?;
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
            ops::laplacian(
                &mut m,
                &mesh,
                &fv,
                &p_bc,
                &r_au_f,
                schemes.pressure_sn_grad.without_correction(),
            )?;
            // The correction is explicit, from the pressure the matrix is
            // assembled with; its flux goes into the face flux as well.
            let corrected = (schemes.pressure_sn_grad == SnGrad::Corrected)
                .then(|| explicit::pressure_laplacian_correction((&mesh, &fv), &p_bc, &p, &r_au_f));
            if let Some(flux) = &corrected {
                correction::subtract_divergence(&mut m.source, &mesh, flux);
            }
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
                let mut flux = ops::laplacian_flux(
                    &mesh,
                    &fv,
                    &p_bc,
                    &r_au_f,
                    schemes.pressure_sn_grad.without_correction(),
                    &p,
                );
                for (x, c) in flux.iter_mut().zip(corrected.iter().flatten()) {
                    *x += c;
                }
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
        if let Some(t) = turbulence.as_mut() {
            solves.extend(t.correct(&mesh, &fv, &u, &u_bc, &phi, &mut log)?);
        }

        let converged = !residuals.is_empty()
            && converged(&residuals, &solves, &["U", "p", "epsilon", "omega", "k"]);
        let written = run.writes(step) || converged;
        if written {
            write_flow(&case.dir, &name, (&mut u_field, &u), (&mut p_field, &p))?;
            flux_field(&mesh, &phi).write(&case.dir, &name)?;
            if let Some(t) = &turbulence {
                t.write(&mesh, &case.dir, &name)?;
            }
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

/// Whether every one of `fields` that `residualControl` names began this
/// iteration below its tolerance; a vector by its largest component.
pub(crate) fn converged(
    control: &[(String, f64)],
    solves: &[(String, Performance)],
    fields: &[&str],
) -> bool {
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
    for &field in fields {
        let Some(tol) = control
            .iter()
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
