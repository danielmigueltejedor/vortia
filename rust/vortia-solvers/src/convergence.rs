//! The automatic stop of `vortiaCentralFoam`: native checks of a steady
//! localEuler run, from the fields and the fluxes, not from the linear
//! solvers' residuals.
//!
//! The engine's counterpart of the solver's `ConvergenceMonitor.H` and
//! `ConvergencePolicy.H` (in `examples/nozzle-video-2026/solver`). Every
//! `interval` iterations the run is sampled: the mass the open patches let
//! in and out, the mass each cell does not balance, how much the fields
//! changed since the last sample and how much of the step's change is left
//! in units of the cell's crossing time. A sample is good when all of them
//! are within their tolerances; the run stops after `stableSamples` good
//! samples in a row once it has run `minimumIterations`. Each sample is
//! reported as a `VORTIA_CONVERGENCE` line and in `vortia-convergence.json`,
//! which the app reads.

use std::path::Path;

use vortia_core::Vec3;
use vortia_foamfile::Dictionary;
use vortia_fv::{FvGeometry, ops};
use vortia_mesh::{PatchKind, PolyMesh};

use crate::error::{Result, SolverError};

/// The limits of `controlDict`'s `vortiaConvergence`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Limits {
    pub(crate) interval: i64,
    pub(crate) minimum_iterations: i64,
    pub(crate) stable_samples: i64,
    pub(crate) mass_tolerance: f64,
    pub(crate) field_tolerance: f64,
    pub(crate) absolute_mass_tolerance: f64,
    pub(crate) equation_tolerance: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            interval: 50,
            minimum_iterations: 500,
            stable_samples: 5,
            mass_tolerance: 1e-3,
            field_tolerance: 1e-3,
            absolute_mass_tolerance: 1e-9,
            equation_tolerance: 1e-5,
        }
    }
}

impl Limits {
    fn valid(&self) -> bool {
        let within = |x: f64, lo: f64, hi: f64| x.is_finite() && x >= lo && x <= hi;
        (1..=10_000).contains(&self.interval)
            && self.minimum_iterations >= self.interval
            && self.minimum_iterations <= 200_000
            && (2..=100).contains(&self.stable_samples)
            && within(self.mass_tolerance, 1e-8, 0.01)
            && within(self.field_tolerance, 1e-8, 0.01)
            && within(self.absolute_mass_tolerance, 1e-15, 1e-6)
            && within(self.equation_tolerance, 1e-8, 1e-3)
    }

    /// The limits and whether reaching them stops the run (`enabled`).
    pub(crate) fn read(control: &Dictionary) -> Result<(Self, bool)> {
        let Some(d) = control.dict("vortiaConvergence") else {
            return Ok((Self::default(), true));
        };
        let base = Self::default();
        let limits = Self {
            interval: d.label("interval").unwrap_or(base.interval),
            minimum_iterations: d
                .label("minimumIterations")
                .unwrap_or(base.minimum_iterations),
            stable_samples: d.label("stableSamples").unwrap_or(base.stable_samples),
            mass_tolerance: d.scalar("massTolerance").unwrap_or(base.mass_tolerance),
            field_tolerance: d.scalar("fieldTolerance").unwrap_or(base.field_tolerance),
            absolute_mass_tolerance: d
                .scalar("absoluteMassTolerance")
                .unwrap_or(base.absolute_mass_tolerance),
            equation_tolerance: d
                .scalar("equationTolerance")
                .unwrap_or(base.equation_tolerance),
        };
        if !limits.valid() {
            return Err(SolverError::Setup(
                "invalid vortiaConvergence controls".into(),
            ));
        }
        let enabled = d
            .word("enabled")
            .is_none_or(|w| !matches!(w, "no" | "off" | "false" | "n" | "none" | "0"));
        Ok((limits, enabled))
    }
}

/// What one sample measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    global_mass: f64,
    local_mass: f64,
    field_change: f64,
    equation_error: f64,
    physical: bool,
    open_boundary: bool,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            global_mass: 0.0,
            local_mass: 0.0,
            field_change: 0.0,
            equation_error: 0.0,
            physical: true,
            open_boundary: true,
        }
    }
}

/// The stability window: samples in a row that were good.
struct Policy {
    limits: Limits,
    stable: i64,
}

impl Policy {
    fn sample(&mut self, elapsed: i64, m: &Metrics) -> bool {
        let l = &self.limits;
        let finite = [
            m.global_mass,
            m.local_mass,
            m.field_change,
            m.equation_error,
        ]
        .iter()
        .all(|x| x.is_finite());
        let good = l.valid()
            && m.physical
            && m.open_boundary
            && finite
            && m.global_mass >= 0.0
            && m.local_mass >= 0.0
            && m.field_change >= 0.0
            && m.global_mass <= l.mass_tolerance
            && m.local_mass <= l.mass_tolerance
            && m.field_change <= l.field_tolerance
            && m.equation_error >= 0.0
            && m.equation_error <= l.equation_tolerance;
        self.stable = if elapsed >= l.minimum_iterations && good {
            self.stable + 1
        } else {
            0
        };
        self.stable >= l.stable_samples
    }
}

/// A scalar field with its boundary values.
pub(crate) type Scalar<'a> = (&'a [f64], &'a [Option<f64>]);

/// The fields of a sample.
pub(crate) struct Fields<'a> {
    pub(crate) p: Scalar<'a>,
    pub(crate) t: Scalar<'a>,
    pub(crate) rho: Scalar<'a>,
    pub(crate) u: (&'a [Vec3], &'a [Option<Vec3>]),
    pub(crate) nu_tilda: Option<Scalar<'a>>,
}

/// What the step started from.
pub(crate) struct Origin<'a> {
    pub(crate) p: &'a [f64],
    pub(crate) t: &'a [f64],
    pub(crate) rho: &'a [f64],
    pub(crate) u: &'a [Vec3],
    /// The speed of sound of the start.
    pub(crate) sound: &'a [f64],
}

pub(crate) struct Monitor {
    policy: Policy,
    automatic: bool,
    old_p: Vec<f64>,
    old_t: Vec<f64>,
    old_rho: Vec<f64>,
    old_nu: Vec<f64>,
    old_u: Vec<Vec3>,
    elapsed: i64,
    checked: f64,
    incoming: f64,
    outgoing: f64,
    reached: bool,
    sampled: bool,
    metrics: Metrics,
    path: std::path::PathBuf,
}

/// A number as the report writes it: 1e300 where it is not finite.
fn finite(x: f64) -> f64 {
    if x.is_finite() { x } else { 1e300 }
}

impl Monitor {
    /// A monitor for a run that starts from `fields` at `time`; replaces
    /// the previous assessment at once with `warming`.
    pub(crate) fn new(
        case: &Path,
        (limits, automatic): (Limits, bool),
        fields: &Fields<'_>,
        time: f64,
        mut log: impl FnMut(&str),
    ) -> Self {
        let monitor = Self {
            policy: Policy { limits, stable: 0 },
            automatic,
            old_p: fields.p.0.to_vec(),
            old_t: fields.t.0.to_vec(),
            old_rho: fields.rho.0.to_vec(),
            old_nu: fields.nu_tilda.map(|n| n.0.to_vec()).unwrap_or_default(),
            old_u: fields.u.0.to_vec(),
            elapsed: 0,
            checked: 0.0,
            incoming: 0.0,
            outgoing: 0.0,
            reached: false,
            sampled: false,
            metrics: Metrics::default(),
            path: case.join("vortia-convergence.json"),
        };
        monitor.report("warming", time, &mut log);
        monitor
    }

    /// Whether the next iteration is one that samples the run, and so
    /// needs what it started from.
    pub(crate) fn due(&self) -> bool {
        (self.elapsed + 1) % self.policy.limits.interval == 0
    }

    /// Counts the iteration that ended at `time` and, every `interval`
    /// iterations, samples the run. `Ok(true)` when it has converged and
    /// stopping on it is on; an error when a field is not finite or a
    /// positive one is not positive.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check(
        &mut self,
        (mesh, fv): (&PolyMesh, &FvGeometry),
        phi: &[f64],
        fields: &Fields<'_>,
        start: Option<Origin<'_>>,
        r_delta_t: &[f64],
        time: f64,
        mut log: impl FnMut(&str),
    ) -> Result<bool> {
        self.elapsed += 1;
        if self.elapsed % self.policy.limits.interval != 0 {
            return Ok(false);
        }
        let Some(start) = start else {
            return Ok(false);
        };
        let mut m = Metrics::default();
        let (mut incoming, mut outgoing, mut patches) = (0.0, 0.0, 0usize);
        for patch in mesh.patches() {
            // Generated open boundaries are generic patches; walls, the
            // axis and the wedge sides are not.
            if patch.kind != PatchKind::Patch {
                continue;
            }
            patches += usize::from(patch.size > 0);
            for f in patch.faces() {
                let flux = phi.get(f).copied().unwrap_or(0.0);
                if !flux.is_finite() {
                    m.physical = false;
                }
                incoming += (-flux).max(0.0);
                outgoing += flux.max(0.0);
            }
        }
        self.incoming = incoming;
        self.outgoing = outgoing;
        m.open_boundary = patches > 0;
        let l = self.policy.limits;
        let scale = incoming
            .max(outgoing)
            .max(l.absolute_mass_tolerance / l.mass_tolerance);
        m.global_mass = (outgoing - incoming).abs() / scale;
        let volumes = fv.volumes();
        let mut local = 0.0;
        for (div, v) in ops::surface_integrate(mesh, fv, phi).iter().zip(volumes) {
            let flux = div.abs() * v;
            if !flux.is_finite() {
                m.physical = false;
            }
            local += flux;
        }
        m.local_mass = local / scale;

        let mut change = 0.0f64;
        let mut scalar = |now: Scalar<'_>, old: &mut Vec<f64>, floor: f64, positive: bool| {
            let mut c = 0.0f64;
            for (&x, &o) in now.0.iter().zip(old.iter()) {
                if !x.is_finite() || (positive && x <= 0.0) {
                    m.physical = false;
                }
                c = c.max((x - o).abs() / floor.max(x.abs().max(o.abs())));
            }
            for x in now.1.iter().flatten() {
                if !x.is_finite() || (positive && *x <= 0.0) {
                    m.physical = false;
                }
            }
            old.clear();
            old.extend_from_slice(now.0);
            c
        };
        change = change.max(scalar(fields.p, &mut self.old_p, 1.0, true));
        change = change.max(scalar(fields.t, &mut self.old_t, 1.0, true));
        change = change.max(scalar(fields.rho, &mut self.old_rho, 1e-9, true));
        if let Some(nu) = fields.nu_tilda
            && !self.old_nu.is_empty()
        {
            change = change.max(scalar(nu, &mut self.old_nu, 1e-8, false));
        }
        for (v, o) in fields.u.0.iter().zip(&self.old_u) {
            if ![v.x, v.y, v.z].iter().all(|x| x.is_finite()) {
                m.physical = false;
            }
            let (vm, om) = (v.norm(), o.norm());
            change = change.max((*v - *o).norm() / 1.0f64.max(vm.max(om)));
        }
        for v in fields.u.1.iter().flatten() {
            if ![v.x, v.y, v.z].iter().all(|x| x.is_finite()) {
                m.physical = false;
            }
        }
        m.field_change = change;

        // The step's change over the accepted local step, in units of the
        // time an acoustic wave takes to cross the cell, so that a tiny step
        // alone cannot look like a steady state.
        let mut error = 0.0f64;
        for (i, (&v, &r)) in volumes.iter().zip(r_delta_t).enumerate() {
            let get = |s: &[f64]| s.get(i).copied().unwrap_or(f64::NAN);
            let (u, u0) = (
                fields.u.0.get(i).copied().unwrap_or_default(),
                start.u.get(i).copied().unwrap_or_default(),
            );
            let speed = 1.0f64.max(get(start.sound) + u.norm());
            let rate = r * (v.cbrt() / speed);
            let du = (u - u0).norm() / speed;
            let rel = |now: f64, then: f64, floor: f64| (now - then).abs() / floor.max(now.abs());
            let dp = rel(get(fields.p.0), get(start.p), 1.0);
            let dt = rel(get(fields.t.0), get(start.t), 1.0);
            let drho = rel(get(fields.rho.0), get(start.rho), 1e-9);
            let e = rate * du.max(dp).max(dt.max(drho));
            if !e.is_finite() || rate <= 0.0 {
                m.physical = false;
            }
            error = error.max(e);
        }
        m.equation_error = error;
        self.old_u.clear();
        self.old_u.extend_from_slice(fields.u.0);
        self.checked = time;
        self.reached = self.policy.sample(self.elapsed, &m);
        self.sampled = true;
        self.metrics = m;
        let status = if !m.physical {
            "invalid"
        } else if self.reached {
            "converged"
        } else if self.elapsed < l.minimum_iterations {
            "warming"
        } else if self.policy.stable > 0 {
            "stabilizing"
        } else {
            "checking"
        };
        self.report(status, time, &mut log);
        if !m.physical {
            return Err(SolverError::Setup(
                "automatic checks detected invalid numerical fields".into(),
            ));
        }
        Ok(self.automatic && self.reached)
    }

    /// The final report: `converged` only when the run ended on a sample
    /// that was.
    pub(crate) fn finish(&self, time: f64, log: impl FnMut(&str)) {
        let status = if self.reached && self.elapsed % self.policy.limits.interval == 0 {
            "converged"
        } else if self.metrics.physical {
            "limit"
        } else {
            "invalid"
        };
        self.report(status, time, log);
    }

    fn report(&self, status: &str, time: f64, mut log: impl FnMut(&str)) {
        let (l, m) = (&self.policy.limits, &self.metrics);
        let json = format!(
            "{{\"schema\":1,\"status\":\"{status}\",\"iteration\":{},\"checkedIteration\":{},\
             \"sampled\":{},\"automatic\":{},\"globalMass\":{},\"localMass\":{},\
             \"massIncoming\":{},\"massOutgoing\":{},\"equationError\":{},\
             \"equationTolerance\":{},\"fieldChange\":{},\"stableSamples\":{},\
             \"requiredSamples\":{},\"massTolerance\":{},\"fieldTolerance\":{},\"interval\":{}}}",
            finite(time),
            finite(self.checked),
            self.sampled,
            self.automatic,
            finite(m.global_mass),
            finite(m.local_mass),
            finite(self.incoming),
            finite(self.outgoing),
            finite(m.equation_error),
            l.equation_tolerance,
            finite(m.field_change),
            self.policy.stable,
            l.stable_samples,
            l.mass_tolerance,
            l.field_tolerance,
            l.interval,
        );
        log(&format!("VORTIA_CONVERGENCE {json}\n"));
        // A report nobody can write is not a reason to stop the run.
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, format!("{json}\n")).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Metrics {
        Metrics::default()
    }

    fn policy() -> Policy {
        Policy {
            limits: Limits {
                interval: 10,
                minimum_iterations: 20,
                stable_samples: 3,
                ..Limits::default()
            },
            stable: 0,
        }
    }

    #[test]
    fn the_run_stops_after_enough_good_samples_in_a_row_once_it_has_run_long_enough() {
        let mut p = policy();
        // Too early: good samples do not count.
        assert!(!p.sample(10, &good()));
        assert_eq!(p.stable, 0);
        assert!(!p.sample(20, &good()));
        assert!(!p.sample(30, &good()));
        assert!(p.sample(40, &good()));
    }

    #[test]
    fn a_bad_sample_starts_the_window_again() {
        let mut p = policy();
        p.sample(20, &good());
        p.sample(30, &good());
        let bad = Metrics {
            equation_error: 1.0,
            ..good()
        };
        assert!(!p.sample(40, &bad));
        assert_eq!(p.stable, 0);
        for m in [
            Metrics {
                global_mass: 0.5,
                ..good()
            },
            Metrics {
                local_mass: 0.5,
                ..good()
            },
            Metrics {
                field_change: 0.5,
                ..good()
            },
            Metrics {
                physical: false,
                ..good()
            },
            Metrics {
                open_boundary: false,
                ..good()
            },
            Metrics {
                field_change: f64::NAN,
                ..good()
            },
        ] {
            assert!(!p.sample(50, &m));
            assert_eq!(p.stable, 0, "{m:?}");
        }
    }

    #[test]
    fn limits_outside_their_range_are_refused() {
        let parse = |text: &str| {
            let src = format!("vortiaConvergence {{ {text} }}");
            Dictionary::parse(
                &mut vortia_foamfile::Tokenizer::new(src.as_bytes()),
                false,
                0,
            )
            .unwrap()
        };
        assert_eq!(Limits::read(&parse("")).unwrap().0, Limits::default());
        let (l, on) =
            Limits::read(&parse("interval 10; minimumIterations 100; enabled no;")).unwrap();
        assert_eq!((l.interval, l.minimum_iterations, on), (10, 100, false));
        for bad in [
            "interval 0;",
            "interval 100000;",
            "minimumIterations 10;",
            "stableSamples 1;",
            "massTolerance 1;",
            "equationTolerance 0;",
            "fieldTolerance 1e-12;",
        ] {
            assert!(Limits::read(&parse(bad)).is_err(), "{bad}");
        }
    }
}
