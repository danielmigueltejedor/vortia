//! Diffusion of a scalar: `∂T/∂t = ∇·(D_T ∇T)`, steady or transient.
//!
//! The engine's counterpart of OpenFOAM's `laplacianFoam` (OpenFOAM,
//! GPL-3.0-or-later), reading the same case: `T` in the start time,
//! `DT` in `constant/transportProperties`, the time loop in `controlDict`, the
//! schemes and solver in `system/`. It writes `T` to the same time
//! directories, so the two runs can be compared file by file.

use vortia_fields::{Data, ScalarField};
use vortia_fv::ops::{self, SnGrad};
use vortia_fv::{Boundary, FvGeometry, LduMatrix, Performance, pcg_dic};
use vortia_mesh::PolyMesh;

use crate::case::{Case, read_dict, time_name};
use crate::error::{Result, SolverError};

/// The most time steps one run may take, so a typo in `deltaT` cannot hang.
pub const MAX_STEPS: u64 = 10_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub time: String,
    pub solves: Vec<Performance>,
    pub written: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ddt {
    Steady,
    Euler,
}

fn diffusivity(case: &Case) -> Result<f64> {
    let props = read_dict(&case.dir.join("constant").join("transportProperties"))?;
    // `DT 4e-05;`, `DT [0 2 -1 0 0 0 0] 4e-05;` and `DT DT [...] 4e-05;` all
    // end in the number.
    props
        .values("DT")
        .and_then(|v| v.last())
        .and_then(|v| v.as_scalar())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(|| SolverError::Setup("transportProperties has no valid 'DT'".into()))
}

/// Runs the case and returns what each time step did. `log` receives the
/// lines OpenFOAM would print.
pub fn run(case: &Case, mut log: impl FnMut(&str)) -> Result<Vec<Step>> {
    let mesh = PolyMesh::read_case(&case.dir)?;
    let fv = FvGeometry::new(&mesh);
    let dt_coeff = diffusivity(case)?;

    let ddt = match case
        .scheme("ddtSchemes", "ddt(T)")?
        .first()
        .map(String::as_str)
    {
        Some("steadyState") => Ddt::Steady,
        Some("Euler") => Ddt::Euler,
        other => {
            return Err(SolverError::Setup(format!(
                "ddt scheme '{}' is not supported yet",
                other.unwrap_or("")
            )));
        }
    };
    let words = case.scheme("laplacianSchemes", "laplacian(DT,T)")?;
    let sn_grad = SnGrad::from_scheme(&words.iter().map(String::as_str).collect::<Vec<_>>())?;
    let (solver, precon, controls) = case.solver_controls("T")?;
    if solver != "PCG" || precon != "DIC" {
        return Err(SolverError::Setup(format!(
            "solver {solver} with {precon} is not supported yet; use PCG with DIC"
        )));
    }
    let n_corr = case.non_orthogonal_correctors("SIMPLE");

    let start_time = match case.control.word("startFrom") {
        Some("latestTime") => vortia_fields::latest(&case.dir)?
            .map(|t| t.value)
            .unwrap_or(0.0),
        _ => case.control.scalar("startTime").unwrap_or(0.0),
    };
    let end_time = case.scalar("endTime")?;
    let delta_t = case.scalar("deltaT")?;
    if !(delta_t > 0.0 && delta_t.is_finite()) {
        return Err(SolverError::Setup("deltaT must be positive".into()));
    }
    let steps_f = ((end_time - start_time) / delta_t).round();
    if !(0.0..=MAX_STEPS as f64).contains(&steps_f) {
        return Err(SolverError::Setup(format!("{steps_f} time steps")));
    }
    let n_steps = steps_f as u64;
    let write_every = match case.control.word("writeControl").unwrap_or("timeStep") {
        "timeStep" => case
            .control
            .scalar("writeInterval")
            .unwrap_or(1.0)
            .round()
            .max(1.0) as u64,
        "runTime" | "adjustableRunTime" => (case.control.scalar("writeInterval").unwrap_or(delta_t)
            / delta_t)
            .round()
            .max(1.0) as u64,
        other => {
            return Err(SolverError::Setup(format!(
                "writeControl '{other}' is not supported yet"
            )));
        }
    };

    let start_name = time_name(start_time);
    let mut field = ScalarField::read(case.dir.join(&start_name).join("T"))?;
    let bc = Boundary::from_field(&field, &mesh)?;
    let mut t = field
        .internal
        .expand(mesh.n_cells())
        .ok_or_else(|| SolverError::Setup("T does not fit the mesh".into()))?;

    let mut steps = Vec::new();
    for step in 1..=n_steps {
        let time = start_time + step as f64 * delta_t;
        let name = time_name(time);
        log(&format!("Time = {name}\n"));
        let old = t.clone();
        let mut solves = Vec::new();
        for _ in 0..=n_corr {
            let mut m = LduMatrix::new(&mesh);
            if ddt == Ddt::Euler {
                ops::ddt_euler(&mut m, &fv, &old, delta_t)?;
            }
            ops::diffusion(&mut m, &mesh, &fv, &bc, dt_coeff, sn_grad, &t)?;
            let perf = pcg_dic(&m, &mut t, &controls)?;
            log(&format!(
                "DICPCG:  Solving for T, Initial residual = {:e}, Final residual = {:e}, No Iterations {}\n",
                perf.initial_residual, perf.final_residual, perf.iterations
            ));
            solves.push(perf);
        }
        let written = step % write_every == 0 || step == n_steps;
        if written {
            field.internal = Data::NonUniform(t.clone());
            field.write(&case.dir, &name)?;
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
