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
    if sn_grad == SnGrad::Corrected {
        // The correction interpolates the cell gradient; the engine has Gauss
        // linear only, so any other choice must not silently become it.
        let grad = case.scheme("gradSchemes", "grad(T)")?;
        if grad != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "grad scheme '{}' is not supported yet; use Gauss linear",
                grad.join(" ")
            )));
        }
    }
    let (solver, precon, controls) = case.solver_controls("T")?;
    if solver != "PCG" || precon != "DIC" {
        return Err(SolverError::Setup(format!(
            "solver {solver} with {precon} is not supported yet; use PCG with DIC"
        )));
    }
    let n_corr = case.non_orthogonal_correctors("SIMPLE");

    let times = vortia_fields::times(&case.dir)?;
    let start_time = match case.control.word("startFrom") {
        Some("latestTime") => times.last().map_or(0.0, |t| t.value),
        _ => case.control.scalar("startTime").unwrap_or(0.0),
    };
    // Read from the directory as it is named on disk: `0.123456789` is not
    // the six-digit name the engine would give that time.
    let start_name = times
        .iter()
        .find(|t| (t.value - start_time).abs() <= 1e-12 * start_time.abs().max(1.0))
        .map_or_else(|| time_name(start_time), |t| t.name.clone());
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
    let write = WriteControl::read(case, delta_t)?;
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
        let written = write.due(step, step as f64 * delta_t, delta_t) || step == n_steps;
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

/// When to write, as `controlDict` asks.
#[derive(Debug, Clone, Copy, PartialEq)]
enum WriteControl {
    /// Every `n` steps.
    Steps(u64),
    /// Every `interval` of simulated time, counted from the start.
    Time(f64),
}

impl WriteControl {
    fn read(case: &Case, delta_t: f64) -> Result<Self> {
        let interval = case.control.scalar("writeInterval");
        let positive = |v: f64| v.is_finite() && v > 0.0;
        match case.control.word("writeControl").unwrap_or("timeStep") {
            "timeStep" => Ok(Self::Steps(
                interval.unwrap_or(1.0).round().clamp(1.0, MAX_STEPS as f64) as u64,
            )),
            control @ ("runTime" | "adjustableRunTime") => {
                let interval = interval.filter(|&v| positive(v)).ok_or_else(|| {
                    SolverError::Setup("writeInterval must be a positive time".into())
                })?;
                // `adjustableRunTime` shortens steps to land on write times;
                // the engine does not, so it accepts only intervals the steps
                // already land on.
                let steps = interval / delta_t;
                if control == "adjustableRunTime" && (steps - steps.round()).abs() > 1e-9 * steps {
                    return Err(SolverError::Setup(
                        "adjustableRunTime with a writeInterval that is not a whole number \
                         of deltaT is not supported yet"
                            .into(),
                    ));
                }
                Ok(Self::Time(interval))
            }
            other => Err(SolverError::Setup(format!(
                "writeControl '{other}' is not supported yet"
            ))),
        }
    }

    /// Whether `step`, at `elapsed` time since the start, writes. A time
    /// interval writes on the first step that reaches each multiple of it,
    /// OpenFOAM's `writeTimeIndex` rule, so it never drifts.
    fn due(self, step: u64, elapsed: f64, delta_t: f64) -> bool {
        match self {
            Self::Steps(n) => step.is_multiple_of(n),
            Self::Time(interval) => {
                let index = |t: f64| ((t + 0.5 * delta_t) / interval).floor();
                index(elapsed) > index(elapsed - delta_t)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WriteControl;

    #[test]
    fn run_time_writes_follow_the_interval_not_a_rounded_step_count() {
        let w = WriteControl::Time(0.25);
        let written: Vec<u64> = (1..=10)
            .filter(|&s| w.due(s, s as f64 * 0.1, 0.1))
            .collect();
        // What OpenFOAM v1912's laplacianFoam writes for this controlDict:
        // 0.2, 0.5, 0.7 and 1, the step nearest each multiple of 0.25.
        assert_eq!(written, [2, 5, 7, 10]);
    }
}
