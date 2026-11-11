//! What every solver reads from `controlDict` and `fvSolution` the same way:
//! the time loop, when to write, and which linear solver to use.
//!
//! The rules follow OpenFOAM's `Time` and `lduMatrix::solver` (OpenFOAM,
//! GPL-3.0-or-later), measured against OpenFOAM v1912 where they are subtle.

use vortia_fv::{
    Controls, LduMatrix, Performance, pbicgstab_dilu, pcg_dic, smooth_sym_gauss_seidel,
};

use crate::case::{Case, DEFAULT_TIME_PRECISION, MAX_TIME_PRECISION, time_name, time_name_with};
use crate::error::{Result, SolverError};

/// The most time steps one run may take, so a typo in `deltaT` cannot hang.
pub const MAX_STEPS: u64 = 10_000_000;

/// The time loop of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunControl {
    pub start_time: f64,
    /// The start directory as it is named on disk.
    pub start_name: String,
    pub delta_t: f64,
    pub n_steps: u64,
    /// Significant digits in the names of the times written.
    pub precision: u32,
    write: WriteControl,
}

impl RunControl {
    pub fn read(case: &Case) -> Result<Self> {
        let times = vortia_fields::times(&case.dir)?;
        let start_time = match case.control.word("startFrom") {
            Some("latestTime") => times.last().map_or(0.0, |t| t.value),
            _ => case.control.scalar("startTime").unwrap_or(0.0),
        };
        // `0.123456789` is not the six-digit name the engine would give that
        // time, so the directory is found by value.
        let start_name = times
            .iter()
            .find(|t| (t.value - start_time).abs() <= 1e-12 * start_time.abs().max(1.0))
            .map_or_else(|| time_name(start_time), |t| t.name.clone());
        let end_time = case.scalar("endTime")?;
        let delta_t = case.scalar("deltaT")?;
        if !(delta_t > 0.0 && delta_t.is_finite()) {
            return Err(SolverError::Setup("deltaT must be positive".into()));
        }
        let steps = ((end_time - start_time) / delta_t).round();
        if !(0.0..=MAX_STEPS as f64).contains(&steps) {
            return Err(SolverError::Setup(format!("{steps} time steps")));
        }
        let n_steps = steps as u64;
        let requested = case
            .control
            .label("timePrecision")
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or(DEFAULT_TIME_PRECISION);
        let precision = distinct_precision(requested, start_time, delta_t, n_steps)?;
        Ok(Self {
            start_time,
            start_name,
            delta_t,
            n_steps,
            precision,
            write: WriteControl::read(case, delta_t)?,
        })
    }

    /// The directory name of the time at the end of `step`.
    pub fn name(&self, step: u64) -> String {
        time_name_with(self.time(step), self.precision)
    }

    /// The time at the end of `step` (counted from 1).
    pub fn time(&self, step: u64) -> f64 {
        self.start_time + step as f64 * self.delta_t
    }

    /// Whether `step` writes. OpenFOAM writes only when one is due, so the
    /// last step of a run is not written unless it falls on one.
    pub fn writes(&self, step: u64) -> bool {
        self.write
            .due(step, step as f64 * self.delta_t, self.delta_t)
    }
}

/// The fewest digits, from `requested` up, that give every time of the run
/// a name of its own, so no write lands on another time's directory (or on
/// the start). OpenFOAM raises `timePrecision` the same way when names
/// collide.
fn distinct_precision(requested: u32, start: f64, dt: f64, n_steps: u64) -> Result<u32> {
    let end = start + n_steps as f64 * dt;
    let largest = start.abs().max(end.abs());
    if largest == 0.0 || n_steps == 0 {
        return Ok(requested.clamp(1, MAX_TIME_PRECISION));
    }
    let exp = largest.log10().floor() as i32;
    // Names a rounding step of no more than `dt` apart stay distinct.
    (requested.clamp(1, MAX_TIME_PRECISION)..=MAX_TIME_PRECISION)
        .find(|&p| 10f64.powi(exp + 1 - p as i32) <= dt * (1.0 + 1e-9))
        .ok_or_else(|| {
            SolverError::Setup(format!(
                "deltaT {dt} is too small to name the times near {largest} apart"
            ))
        })
}

/// The linear solvers the engine has, by their `fvSolution` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearSolver {
    PcgDic,
    PbicgstabDilu,
    /// `smoothSolver` with `symGaussSeidel`, `nSweeps` per check.
    SymGaussSeidel(usize),
}

impl LinearSolver {
    /// The solver and controls `fvSolution` gives `field`.
    pub fn read(case: &Case, field: &str) -> Result<(Self, Controls)> {
        let (solver, precon, controls) = case.solver_controls(field)?;
        let s = match (solver.as_str(), precon.as_str()) {
            ("PCG", "DIC") => Self::PcgDic,
            ("PBiCGStab", "DILU") => Self::PbicgstabDilu,
            ("smoothSolver", "symGaussSeidel") => {
                let sweeps = case
                    .solver_dict(field)?
                    .label("nSweeps")
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(1)
                    .clamp(1, 1000);
                Self::SymGaussSeidel(sweeps)
            }
            _ => {
                return Err(SolverError::Setup(format!(
                    "solver {solver} with {precon} for '{field}' is not supported yet; \
                     use PCG with DIC, PBiCGStab with DILU or smoothSolver with symGaussSeidel"
                )));
            }
        };
        Ok((s, controls))
    }

    pub fn solve(self, m: &LduMatrix, x: &mut [f64], c: &Controls) -> Result<Performance> {
        Ok(match self {
            Self::PcgDic => pcg_dic(m, x, c)?,
            Self::PbicgstabDilu => pbicgstab_dilu(m, x, c)?,
            Self::SymGaussSeidel(sweeps) => smooth_sym_gauss_seidel(m, x, c, sweeps)?,
        })
    }
}

/// The line OpenFOAM logs for a solve.
pub fn solve_line(field: &str, perf: &Performance) -> String {
    format!(
        "{}:  Solving for {field}, Initial residual = {:e}, Final residual = {:e}, No Iterations {}\n",
        perf.solver, perf.initial_residual, perf.final_residual, perf.iterations
    )
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
    use super::{WriteControl, distinct_precision};
    use crate::case::time_name_with;

    #[test]
    fn time_names_gain_digits_rather_than_collide() {
        // The tutorial's times keep OpenFOAM's six digits.
        assert_eq!(distinct_precision(6, 0.0, 0.005, 100).unwrap(), 6);
        // A restart at 50000 by 0.01 would name its first step "50000" again.
        let p = distinct_precision(6, 50000.0, 0.01, 100).unwrap();
        let names: Vec<_> = (0..=100)
            .map(|k| time_name_with(50000.0 + k as f64 * 0.01, p))
            .collect();
        let mut unique = names.clone();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        assert!(distinct_precision(6, 1e10, 1e-9, 10).is_err());
    }

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
