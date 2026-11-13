//! The convergence report the engine writes while a `vortiaCentralFoam` case runs.

use serde_json::Value;

/// The prefix of the log line that carries the report.
pub const LOG_PREFIX: &str = "VORTIA_CONVERGENCE ";

/// One report. A number the engine could not give (not finite) is `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct Convergence {
    /// `warming`, `checking`, `stabilizing`, `converged`, `limit` or `invalid`.
    pub status: String,
    /// The simulated time of the report.
    pub time: Option<f64>,
    /// The step the last check was made at.
    pub checked_iteration: Option<f64>,
    pub global_mass: Option<f64>,
    pub local_mass: Option<f64>,
    pub equation_error: Option<f64>,
    pub equation_tolerance: Option<f64>,
    pub field_change: Option<f64>,
    pub mass_tolerance: Option<f64>,
    pub field_tolerance: Option<f64>,
    /// Checks in a row that stayed inside the limits, and how many end the run.
    pub stable_samples: u32,
    pub required_samples: u32,
    /// Whether the report may stop the run.
    pub automatic: bool,
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64().filter(|n| n.is_finite())
}

fn count(value: &Value, key: &str) -> u32 {
    number(value, key).map_or(0, |n| n.clamp(0.0, f64::from(u32::MAX)) as u32)
}

impl Convergence {
    /// Reads the JSON of a report; `None` when it is not one.
    pub fn parse(json: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(json.trim()).ok()?;
        Some(Self {
            status: value.get("status")?.as_str()?.to_owned(),
            time: number(&value, "iteration"),
            checked_iteration: number(&value, "checkedIteration"),
            global_mass: number(&value, "globalMass"),
            local_mass: number(&value, "localMass"),
            equation_error: number(&value, "equationError"),
            equation_tolerance: number(&value, "equationTolerance"),
            field_change: number(&value, "fieldChange"),
            mass_tolerance: number(&value, "massTolerance"),
            field_tolerance: number(&value, "fieldTolerance"),
            stable_samples: count(&value, "stableSamples"),
            required_samples: count(&value, "requiredSamples"),
            automatic: value
                .get("automatic")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// Reads a log line when it is a report.
    pub fn from_log_line(line: &str) -> Option<Self> {
        Self::parse(line.trim_start().strip_prefix(LOG_PREFIX)?)
    }

    /// Whether the run is settled.
    pub fn is_converged(&self) -> bool {
        self.status == "converged"
    }

    /// How far the run is towards stopping by itself, from 0 to 1.
    pub fn progress(&self) -> f32 {
        if self.is_converged() {
            return 1.0;
        }
        if self.required_samples == 0 {
            return 0.0;
        }
        (self.stable_samples as f32 / self.required_samples as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"{"schema":1,"status":"stabilizing","iteration":0.5,"checkedIteration":250,"sampled":true,"automatic":true,"globalMass":1e-5,"localMass":null,"massIncoming":1,"massOutgoing":1,"equationError":2e-4,"equationTolerance":1e-3,"fieldChange":3e-6,"stableSamples":2,"requiredSamples":5,"massTolerance":1e-3,"fieldTolerance":1e-4,"interval":50}"#;

    #[test]
    fn a_report_is_read_and_missing_numbers_stay_missing() {
        let r = Convergence::parse(REPORT).expect("a report");
        assert_eq!(r.status, "stabilizing");
        assert_eq!(r.checked_iteration, Some(250.0));
        assert_eq!(r.local_mass, None);
        assert_eq!(r.global_mass, Some(1e-5));
        assert!(r.automatic);
        assert!((r.progress() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn only_a_prefixed_line_is_a_report() {
        assert!(Convergence::from_log_line(&format!("VORTIA_CONVERGENCE {REPORT}\n")).is_some());
        assert!(Convergence::from_log_line(REPORT).is_none());
        assert!(Convergence::from_log_line("Time = 0.5").is_none());
        assert!(Convergence::from_log_line("VORTIA_CONVERGENCE not json").is_none());
        assert!(Convergence::parse("{\"schema\":1}").is_none());
    }

    #[test]
    fn hostile_counts_do_not_wrap_and_a_settled_run_is_full() {
        let r = Convergence::parse(
            r#"{"status":"converged","stableSamples":-3,"requiredSamples":1e30}"#,
        )
        .expect("a report");
        assert_eq!(r.stable_samples, 0);
        assert_eq!(r.required_samples, u32::MAX);
        assert_eq!(r.progress(), 1.0);
    }
}
