//! Steady natural and mixed convection of a Boussinesq fluid:
//! `∇·(φ U) − ∇·(ν ∇U) − … = −∇p_rgh − gh ∇ρ_k`, `∇·(φ T) − ∇·(α ∇T) = 0`,
//! `ρ_k = 1 − β (T − T_ref)`, `∇·U = 0`.
//!
//! The engine's counterpart of OpenFOAM's `buoyantBoussinesqSimpleFoam`
//! (OpenFOAM, GPL-3.0-or-later), laminar. The pressure it solves for is
//! `p_rgh = p − ρ_k gh`, which takes the hydrostatic part out of the problem,
//! so a closed cavity needs `fixedFluxPressure` on its walls: the gradient
//! that makes the flux through them the one the buoyancy asks for. Each
//! iteration solves the momentum equation (with `reconstruct` of the buoyancy
//! and pressure fluxes as its source), the temperature equation with the flux
//! of the last iteration, and then the pressure equation, as `simpleFoam`
//! does; the run stops early when every field in `SIMPLE/residualControl`
//! starts its iteration below its tolerance.
//!
//! It reads `U`, `p_rgh` and `T` (and `p`, which it writes back as
//! `p_rgh + ρ_k gh` when it is there), `constant/g` with an optional
//! `constant/hRef`, and from `constant/transportProperties` `nu`, `beta`,
//! `TRef` and `Pr`; it writes them back with `phi`.

use vortia_core::Vec3;
use vortia_fields::{Data, ScalarField};
use vortia_fv::limited;
use vortia_fv::ops::{self, SnGrad};
use vortia_fv::surface::{reconstruct, sn_grad};
use vortia_fv::vector::{dot_interpolate, solution_directions};
use vortia_fv::{Boundary, LduMatrix, Performance, ScalarBc, ScalarMatrix};
use vortia_mesh::PolyMesh;

use crate::case::{Case, KINEMATIC_DIFFUSIVITY, check_dimensions, read_dict};
use crate::control::{LinearSolver, RunControl, solve_line};
use crate::error::{Result, SolverError};
use crate::explicit;
use crate::ico::{
    Algorithm, FlowFields, Step, adjust_phi, constrained_boundary, extrapolated, flux_field,
    switch, write_flow,
};
use crate::laplacian::transport_property;
use crate::rans::Model;
use crate::simple::{
    Relaxation, Schemes, Viscosity, check_models, converged, momentum_equation, residual_control,
};

const TEMPERATURE: [i32; 7] = [0, 0, 0, 1, 0, 0, 0];
const THERMAL_EXPANSION: [i32; 7] = [0, 0, 0, -1, 0, 0, 0];
const NO_DIMENSIONS: [i32; 7] = [0; 7];

/// Gravity, and the height the hydrostatic pressure is measured from.
struct Gravity {
    g: Vec3,
    /// `ghRef = −|g| hRef`.
    gh_ref: f64,
}

impl Gravity {
    fn read(case: &Case) -> Result<Self> {
        let constant = case.dir.join("constant");
        let dict = read_dict(&constant.join("g"))?;
        let components: Option<Vec<f64>> = dict
            .values("value")
            .and_then(|v| {
                v.iter().find_map(|v| match v.as_list() {
                    Some(items) if items.len() == 3 => Some(items),
                    _ => None,
                })
            })
            .and_then(|items| items.iter().map(|v| v.as_scalar()).collect());
        let g = match components.as_deref() {
            Some(&[x, y, z]) if [x, y, z].iter().all(|c| c.is_finite()) => Vec3::new(x, y, z),
            _ => {
                return Err(SolverError::Setup(
                    "constant/g has no 'value' of three numbers".into(),
                ));
            }
        };
        let h_ref = if constant.join("hRef").is_file() || constant.join("hRef.gz").is_file() {
            read_dict(&constant.join("hRef"))?
                .values("value")
                .and_then(|v| v.last())
                .and_then(|v| v.as_scalar())
                .filter(|h| h.is_finite())
                .ok_or_else(|| SolverError::Setup("constant/hRef has no 'value'".into()))?
        } else {
            0.0
        };
        Ok(Self {
            g,
            gh_ref: -g.norm() * h_ref,
        })
    }
}

/// The face-normal gradient schemes the buoyancy asks for.
struct Gradients {
    temperature: SnGrad,
    pressure: SnGrad,
}

/// The patches whose pressure gradient the solver sets (`fixedFluxPressure`).
fn flux_patches(field: &ScalarField, mesh: &PolyMesh) -> Vec<usize> {
    mesh.patches()
        .iter()
        .enumerate()
        .filter(|(_, patch)| {
            field
                .patch_field(patch)
                .is_some_and(|pf| pf.kind == "fixedFluxPressure")
        })
        .map(|(i, _)| i)
        .collect()
}

/// Runs the case and returns what each iteration did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let (mesh, fv) = case.mesh()?;
    check_models(case)?;
    if !matches!(Model::read(case)?, Model::Laminar) {
        return Err(SolverError::Setup(
            "buoyantBoussinesqSimpleFoam runs laminar only for now: \
             set simulationType laminar in constant/turbulenceProperties"
                .into(),
        ));
    }
    let nu = transport_property(case, "nu", KINEMATIC_DIFFUSIVITY)?;
    let beta = transport_property(case, "beta", THERMAL_EXPANSION)?;
    let t_ref = transport_property(case, "TRef", TEMPERATURE)?;
    let pr = transport_property(case, "Pr", NO_DIMENSIONS)?;
    if pr <= 0.0 {
        return Err(SolverError::Setup(
            "transportProperties: Pr must be positive".into(),
        ));
    }
    let alpha = nu / pr;
    let gravity = Gravity::read(case)?;

    let schemes = Schemes::read(case, "p_rgh")?;
    case.require_gauss_linear_grad("T")?;
    let t_convection = case.convection_with_grad("div(phi,T)")?;
    let t_laplacian = case.uncorrected_sn_grad("laplacian(alphaEff,T)")?;
    let sn = Gradients {
        temperature: case.face_sn_grad("snGrad(rhok)")?,
        pressure: case.face_sn_grad("snGrad(p_rgh)")?,
    };
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
    let (t_solver, t_controls) = LinearSolver::read(case, "T")?;
    let (p_solver, p_controls) = LinearSolver::read(case, "p_rgh")?;
    let run = RunControl::read(case)?;

    let start = case.dir.join(&run.start_name);
    let FlowFields {
        mut u_field,
        p_field: mut p_rgh_field,
        u_bc,
        mut p_bc,
        mut u,
        p: mut p_rgh,
        mut phi,
        need_reference,
    } = FlowFields::read_pressure(&start, &mesh, &fv, "p_rgh", &["fixedFluxPressure"])?;
    let fixed_flux = flux_patches(&p_rgh_field, &mesh);
    let mut t_field = ScalarField::read(start.join("T"))?;
    check_dimensions("T", &t_field.dimensions, TEMPERATURE)?;
    let t_bc = Boundary::from_field(&t_field, &mesh)?;
    let mut t = t_field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("T does not fit the mesh".into()))?;
    // `p` is optional input; when there, it is written as `p_rgh + ρ_k gh`.
    let mut p_field = if start.join("p").is_file() || start.join("p.gz").is_file() {
        Some(ScalarField::read(start.join("p"))?)
    } else {
        None
    };

    let valid = solution_directions(&mesh, &fv);
    let names = ["Ux", "Uy", "Uz"];
    let alpha_u = relaxation.equation("U");
    let alpha_t = relaxation.equation("T");
    let alpha_p = relaxation.field("p_rgh");
    let n_boundary = mesh.n_faces().saturating_sub(mesh.n_internal_faces());
    let nu_cells = vec![nu; mesh.n_cells()];
    let nu_boundary = vec![Some(nu); n_boundary];
    let nu_faces = vec![nu; mesh.n_faces()];
    let alpha_faces = vec![alpha; mesh.n_faces()];
    // `gh` in the cells and `ghf` on the faces.
    let height = |c: &Vec3| gravity.g.dot(*c) - gravity.gh_ref;
    let gh: Vec<f64> = fv.geo.cell_centres.iter().map(height).collect();
    let ghf: Vec<f64> = fv.geo.face_centres.iter().map(height).collect();

    // `−ghf snGrad(ρ_k) |Sf|`, which is `ghf β snGrad(T) |Sf|`.
    let buoyancy = |t: &[f64]| -> Vec<f64> {
        let grad_t = sn_grad(&mesh, &fv, &t_bc, t, sn.temperature);
        grad_t
            .iter()
            .zip(&ghf)
            .zip(&fv.mag_sf)
            .map(|((&g, &h), &a)| h * beta * g * a)
            .collect()
    };

    // The pressure gradient of the `fixedFluxPressure` patches that makes
    // the flux through them `flux`: `(φ_HbyA − Sf·U_b) / (|Sf| rAUf)`.
    let constrain = |bc: &mut Boundary, phi_hbya: &[f64], phi_u: &[f64], r_au_f: &[f64]| {
        for &i in &fixed_flux {
            let (Some(patch), Some(ScalarBc::FixedGradient(g))) =
                (mesh.patches().get(i), bc.conditions.get_mut(i))
            else {
                continue;
            };
            for (value, f) in g.iter_mut().zip(patch.faces()) {
                let (Some(&h), Some(&w), Some(&a), Some(&r)) = (
                    phi_hbya.get(f),
                    phi_u.get(f),
                    fv.mag_sf.get(f),
                    r_au_f.get(f),
                ) else {
                    continue;
                };
                let d = a * r;
                *value = if d > 0.0 { (h - w) / d } else { 0.0 };
            }
        }
    };

    // The gradient the first momentum predictor sees is the one that holds
    // the walls: the buoyancy's own.
    {
        let b = buoyancy(&t);
        let phi_u = dot_interpolate(&mesh, &fv, &u, &u_bc.face_values(&mesh, &u));
        let ones = vec![1.0; mesh.n_faces()];
        constrain(&mut p_bc, &b, &phi_u, &ones);
    }

    let mut steps = Vec::new();
    for step in 1..=run.n_steps {
        crate::cancel::check()?;
        let name = run.name(step);
        log(&format!("Time = {name}\n\n"));
        let p_prev = p_rgh.clone();
        let mut solves: Vec<(String, Performance)> = Vec::new();

        // The momentum equation, with the buoyancy and the pressure
        // gradient reconstructed from their face fluxes as its source.
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
            let grad_p = sn_grad(&mesh, &fv, &p_bc, &p_rgh, sn.pressure);
            let force: Vec<f64> = buoyancy(&t)
                .iter()
                .zip(&grad_p)
                .zip(&fv.mag_sf)
                .map(|((&b, &g), &a)| b - g * a)
                .collect();
            let extra: Vec<Vec3> = reconstruct(&mesh, &fv, &force)
                .into_iter()
                .zip(fv.volumes())
                .map(|(f, &v)| f * v)
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

        // The temperature, carried by the flux of the last iteration.
        let mut t_eqn =
            ScalarMatrix::convection(&mesh, &fv, &t_bc, &phi, t_convection.interpolation)?;
        if t_convection.bounded {
            t_eqn.bound_convection(&mesh, &fv, &phi);
        }
        // `linearUpwind`: the matrix holds the upwind part; the gradient's
        // correction of each face value goes to the source.
        if let Some(scheme) = t_convection.interpolation.grad_scheme() {
            let g = ops::grad_with(scheme, &mesh, &fv, &t_bc, &t);
            let corr = limited::correction_from(&mesh, &fv, &phi, &g);
            for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
                let (Some(&x), Some(&c)) = (phi.get(f), corr.get(f)) else {
                    continue;
                };
                if let Some(s) = t_eqn.ldu.source.get_mut(o) {
                    *s -= c * x;
                }
                if let Some(s) = t_eqn.ldu.source.get_mut(n) {
                    *s += c * x;
                }
            }
        }
        t_eqn.sub(&ScalarMatrix::laplacian(
            &mesh,
            &fv,
            &t_bc,
            &alpha_faces,
            t_laplacian,
        )?);
        if let Some(a) = alpha_t {
            t_eqn.relax(&mesh, &t, a);
        }
        let perf = t_eqn.solve(&mesh, &mut t, |m, x| t_solver.solve(m, x, &t_controls))?;
        log(&solve_line("T", &perf));
        solves.push(("T".to_owned(), perf));

        // The pressure equation, `pEqn.H`.
        let r_au: Vec<f64> = u_eqn.a(&mesh, &fv).iter().map(|&a| 1.0 / a).collect();
        let h = u_eqn.h(&mesh, &fv, &u, valid);
        let hbya: Vec<Vec3> = h.iter().zip(&r_au).map(|(&h, &r)| h * r).collect();
        let hbya_b = constrained_boundary(&mesh, &u_bc, &u, &hbya);
        let r_au_f = ops::interpolate_linear(&mesh, &fv, &r_au, &extrapolated(&mesh, &r_au));
        let buoyancy_flux = buoyancy(&t);
        let phig: Vec<f64> = buoyancy_flux
            .iter()
            .zip(&r_au_f)
            .map(|(&b, &r)| b * r)
            .collect();
        let mut phi_hbya = dot_interpolate(&mesh, &fv, &hbya, &hbya_b);
        if need_reference {
            adjust_phi(&mesh, &u_bc, &mut phi_hbya)?;
        }
        for (x, &g) in phi_hbya.iter_mut().zip(&phig) {
            *x += g;
        }
        let phi_u = dot_interpolate(&mesh, &fv, &u, &u_bc.face_values(&mesh, &u));
        constrain(&mut p_bc, &phi_hbya, &phi_u, &r_au_f);

        let mut p_flux = vec![0.0; mesh.n_faces()];
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
            let corrected = (schemes.pressure_sn_grad == SnGrad::Corrected).then(|| {
                explicit::pressure_laplacian_correction((&mesh, &fv), &p_bc, &p_rgh, &r_au_f)
            });
            if let Some(flux) = &corrected {
                vortia_fv::correction::subtract_divergence(&mut m.source, &mesh, flux);
            }
            let div = ops::surface_integrate(&mesh, &fv, &phi_hbya);
            for ((s, &d), &v) in m.source.iter_mut().zip(&div).zip(fv.volumes()) {
                *s += v * d;
            }
            if need_reference {
                let level = p_rgh.get(simple.ref_cell).copied().unwrap_or(0.0);
                ops::set_reference(&mut m, simple.ref_cell, level);
            }
            let perf = p_solver.solve(&m, &mut p_rgh, &p_controls)?;
            log(&solve_line("p_rgh", &perf));
            solves.push(("p_rgh".to_owned(), perf));
            if non_orth == simple.n_non_orth {
                p_flux = ops::laplacian_flux(
                    &mesh,
                    &fv,
                    &p_bc,
                    &r_au_f,
                    schemes.pressure_sn_grad.without_correction(),
                    &p_rgh,
                );
                for (x, c) in p_flux.iter_mut().zip(corrected.iter().flatten()) {
                    *x += c;
                }
                for ((f, &a), &b) in phi.iter_mut().zip(&phi_hbya).zip(&p_flux) {
                    *f = a - b;
                }
            }
        }

        if let Some(a) = alpha_p {
            for (pc, &prev) in p_rgh.iter_mut().zip(&p_prev) {
                *pc = prev + a * (*pc - prev);
            }
        }
        // The velocity from the flux the pressure just solved, not the
        // relaxed pressure's.
        let corrected_flux: Vec<f64> = phig
            .iter()
            .zip(&p_flux)
            .zip(&r_au_f)
            .map(|((&g, &q), &r)| if r > 0.0 { (g - q) / r } else { 0.0 })
            .collect();
        let rec = reconstruct(&mesh, &fv, &corrected_flux);
        for (((uc, &hb), &r), &c) in u.iter_mut().zip(&hbya).zip(&r_au).zip(&rec) {
            *uc = hb + c * r;
        }

        // `p = p_rgh + ρ_k gh`, and the level of `p_rgh` set from the
        // reference pressure when nothing fixes it.
        let rhok: Vec<f64> = t.iter().map(|&x| 1.0 - beta * (x - t_ref)).collect();
        if need_reference {
            let level = |p: &[f64]| {
                p.get(simple.ref_cell).copied().unwrap_or(0.0)
                    + rhok.get(simple.ref_cell).copied().unwrap_or(1.0)
                        * gh.get(simple.ref_cell).copied().unwrap_or(0.0)
            };
            let shift = simple.ref_value - level(&p_rgh);
            for x in &mut p_rgh {
                *x += shift;
            }
        }

        let converged =
            !residuals.is_empty() && converged(&residuals, &solves, &["U", "p_rgh", "T"]);
        let written = run.writes(step) || converged;
        if written {
            let p_b = p_bc.face_values(&mesh, &p_rgh, &fv.non_orth_delta_coeffs);
            p_rgh_field.store_patch_values(&mesh, &["fixedFluxPressure"], &p_b);
            write_flow(
                &case.dir,
                &name,
                (&mut u_field, &u),
                (&mut p_rgh_field, &p_rgh),
            )?;
            t_field.internal = Data::NonUniform(t.clone());
            t_field.drop_zero_gradient_values();
            t_field.write(&case.dir, &name)?;
            if let Some(field) = p_field.as_mut() {
                let t_b = t_bc.face_values(&mesh, &t, &fv.non_orth_delta_coeffs);
                let n_internal = mesh.n_internal_faces();
                let p_total: Vec<f64> = p_rgh
                    .iter()
                    .zip(&rhok)
                    .zip(&gh)
                    .map(|((&p, &r), &g)| p + r * g)
                    .collect();
                let total_b: Vec<Option<f64>> = p_b
                    .iter()
                    .zip(&t_b)
                    .enumerate()
                    .map(|(k, (p, t))| {
                        let g = ghf.get(n_internal + k).copied()?;
                        Some(p.as_ref()? + (1.0 - beta * (t.as_ref()? - t_ref)) * g)
                    })
                    .collect();
                field.internal = Data::NonUniform(p_total);
                field.store_patch_values(&mesh, &["calculated"], &total_b);
                field.write(&case.dir, &name)?;
            }
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
