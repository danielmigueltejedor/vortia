//! What the flow does to a wall: shear stress, `y+`, the force on it and the
//! heat that passes through it.
//!
//! Definitions follow OpenFOAM's `wallShearStress`, `yPlus`, `forces` and
//! `wallHeatFlux` function objects as they come out for a wall-function
//! mesh: the stress is the effective viscosity times the velocity gradient
//! across the first cell, `nuEff × (U_wall − U_cell) / d` projected on the wall
//! (`d` the distance from the cell centre to the wall along its normal), so it
//! is negative for flow in the positive direction, as OpenFOAM reports it.

use vortia_core::Vec3;
use vortia_foamfile::{FoamFile, Value};

use crate::case::{Case, FieldValues};
use crate::error::{PostError, Result};

/// What the wall's flow solver means by viscosity.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fluid {
    /// `nu` from `transportProperties`: stresses are per unit density.
    Kinematic { nu: f64 },
    /// `mu`, `Cp` and `Pr` of a constant-property gas, with `rho` from the fields.
    Gas { mu: f64, cp: f64, pr: f64 },
}

/// A patch seen as a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct WallReport {
    pub patch: String,
    /// Stresses and forces are per unit density (m²/s², m³/s²) rather than
    /// Pa and N: the solver is incompressible and its pressure is `p/ρ`.
    pub kinematic: bool,
    pub area: f64,
    /// The stress by the wall on the fluid on each face (OpenFOAM's
    /// `wallShearStress`).
    pub shear: Vec<Vec3>,
    /// `y u_τ / ν` on each face, `u_τ` from the stress above; `NaN` on a face
    /// with no first cell to measure.
    pub y_plus: Vec<f64>,
    /// Heat into the fluid per area (W/m²) on each face, positive when the
    /// wall is hotter than the fluid. Only for a gas with a temperature.
    pub heat_flux: Option<Vec<f64>>,
    /// The area of each face, in face order.
    pub areas: Vec<f64>,
    /// `−∑ shear × area`: the viscous force the fluid puts on the wall.
    pub viscous_force: Vec3,
    /// `∑ p × area vector`: the pressure force on the wall; `None` without `p`.
    pub pressure_force: Option<Vec3>,
}

impl WallReport {
    /// The area-weighted mean stress.
    pub fn mean_shear(&self) -> Vec3 {
        if self.area > 0.0 {
            self.viscous_force * (-1.0 / self.area)
        } else {
            Vec3::default()
        }
    }

    /// Heat through the wall per second: `∑ q × area`, positive into the fluid.
    pub fn heat_rate(&self) -> Option<f64> {
        let q = self.heat_flux.as_ref()?;
        Some(q.iter().zip(&self.areas).map(|(q, a)| q * a).sum())
    }

    /// Smallest, mean (over faces) and largest `y+` of the faces that have one.
    pub fn y_plus_range(&self) -> Option<(f64, f64, f64)> {
        let v: Vec<f64> = self
            .y_plus
            .iter()
            .copied()
            .filter(|y| y.is_finite())
            .collect();
        let min = v.iter().copied().reduce(f64::min)?;
        let max = v.iter().copied().reduce(f64::max)?;
        Some((min, v.iter().sum::<f64>() / v.len() as f64, max))
    }
}

impl Case {
    /// Shear stress, `y+`, forces and heat flux on a patch at `time`.
    pub fn wall(&self, time: &str, patch_name: &str) -> Result<WallReport> {
        let patch = self
            .mesh()
            .patch(patch_name)
            .ok_or_else(|| PostError::Invalid(format!("no patch '{patch_name}'")))?
            .clone();
        let fluid = self.fluid()?;
        let FieldValues::Vector(u_cells) = self.field(time, "U")? else {
            return Err(PostError::Invalid("U is not a vector field".into()));
        };
        let FieldValues::Vector(u_wall) = self.patch_field(time, "U", patch_name)? else {
            return Err(PostError::Invalid("U is not a vector field".into()));
        };
        let scalar = |name: &str| -> Result<Option<(Vec<f64>, Vec<f64>)>> {
            if self.field_names(time)?.iter().all(|n| n != name) {
                return Ok(None);
            }
            match (
                self.field(time, name)?,
                self.patch_field(time, name, patch_name)?,
            ) {
                (FieldValues::Scalar(c), FieldValues::Scalar(w)) => Ok(Some((c, w))),
                _ => Ok(None),
            }
        };
        let nut = scalar("nut")?;
        let alphat = scalar("alphat")?;
        let temperature = scalar("T")?;
        let rho = scalar("rho")?;
        if matches!(fluid, Fluid::Gas { .. }) && rho.is_none() {
            return Err(PostError::Invalid(
                "a gas needs its density: the time has no 'rho'".into(),
            ));
        }
        let pressure = scalar("p")?;

        let n = patch.size;
        let (mut shear, mut y_plus) = (Vec::with_capacity(n), Vec::with_capacity(n));
        let mut areas = Vec::with_capacity(n);
        let mut heat = Vec::with_capacity(n);
        let (mut area, mut viscous, mut pressure_force) = (0.0, Vec3::default(), Vec3::default());
        for (k, face) in patch.faces().enumerate() {
            let get = |v: &[Vec3]| v.get(k).copied();
            let (Some(&sf), Some(&fc), Some(&cell)) = (
                self.geometry().face_areas.get(face),
                self.geometry().face_centres.get(face),
                self.mesh().owner().get(face),
            ) else {
                return Err(PostError::Invalid("a wall face has no geometry".into()));
            };
            let (Some(&cc), Some(uc), Some(uw)) = (
                self.geometry().cell_centres.get(cell),
                u_cells.get(cell).copied(),
                get(&u_wall),
            ) else {
                return Err(PostError::Invalid("a wall face has no velocity".into()));
            };
            let magnitude = sf.norm();
            area += magnitude;
            areas.push(magnitude);
            if let Some((_, w)) = &pressure
                && let Some(p) = w.get(k)
            {
                pressure_force = pressure_force + sf * *p;
            }
            let normal = if magnitude > 0.0 {
                sf / magnitude
            } else {
                Vec3::default()
            };
            let d = (fc - cc).dot(normal);
            let at = |s: &Option<(Vec<f64>, Vec<f64>)>| -> (f64, f64) {
                s.as_ref().map_or((0.0, 0.0), |(c, w)| {
                    (
                        c.get(cell).copied().unwrap_or(0.0),
                        w.get(k).copied().unwrap_or(0.0),
                    )
                })
            };
            let (_, nut_w) = at(&nut);
            let (_, rho_w) = at(&rho);
            let (nu_lam, scale) = match fluid {
                Fluid::Kinematic { nu } => (nu, 1.0),
                Fluid::Gas { mu, .. } if rho_w > 0.0 => (mu / rho_w, rho_w),
                Fluid::Gas { .. } => (f64::NAN, 0.0),
            };
            if d.is_nan() || d <= 0.0 || !nu_lam.is_finite() {
                shear.push(Vec3::default());
                y_plus.push(f64::NAN);
                heat.push(0.0);
                continue;
            }
            let jump = uw - uc;
            let tangential = jump - normal * jump.dot(normal);
            let nu_eff = nu_lam + nut_w;
            let tau = tangential * (nu_eff / d);
            // Per unit density; a gas multiplies by its density at the wall.
            let stress = tau * scale;
            viscous = viscous - stress * magnitude;
            y_plus.push(d * (tau.norm()).sqrt() / nu_lam);
            shear.push(stress);
            if let Fluid::Gas { mu, cp, pr } = fluid {
                let (_, t_w) = at(&temperature);
                let (t_c, _) = at(&temperature);
                let (_, alphat_w) = at(&alphat);
                heat.push(cp * (mu / pr + alphat_w) * (t_w - t_c) / d);
            } else {
                heat.push(0.0);
            }
        }
        let gas = matches!(fluid, Fluid::Gas { .. });
        Ok(WallReport {
            patch: patch.name,
            kinematic: !gas,
            area,
            areas,
            shear,
            y_plus,
            heat_flux: (gas && temperature.is_some()).then_some(heat),
            viscous_force: viscous,
            pressure_force: pressure.is_some().then_some(pressure_force),
        })
    }

    /// The viscosity the case's solver uses, from `constant`.
    fn fluid(&self) -> Result<Fluid> {
        let constant = self.path().join("constant");
        let read = |name: &str| -> Option<vortia_foamfile::Dictionary> {
            FoamFile::open(constant.join(name))
                .ok()
                .and_then(|f| f.dictionary().ok())
                .map(|(_, d)| d)
        };
        let number = |d: &vortia_foamfile::Dictionary, key: &str| {
            d.values(key)
                .and_then(|v| v.last())
                .and_then(Value::as_scalar)
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        if let Some(d) = read("transportProperties")
            && let Some(nu) = number(&d, "nu")
        {
            return Ok(Fluid::Kinematic { nu });
        }
        if let Some(d) = read("thermophysicalProperties")
            && let Some(m) = d.dict("mixture")
        {
            let transport = m.dict("transport");
            let mu = transport.and_then(|t| number(t, "mu"));
            let pr = transport.and_then(|t| number(t, "Pr")).filter(|p| *p > 0.0);
            let cp = m.dict("thermodynamics").and_then(|t| number(t, "Cp"));
            if let (Some(mu), Some(pr), Some(cp)) = (mu, pr, cp) {
                return Ok(Fluid::Gas { mu, cp, pr });
            }
        }
        Err(PostError::Invalid(
            "the viscosity is unknown: constant has no transportProperties 'nu' or \
             thermophysicalProperties mixture with mu, Pr and Cp"
                .into(),
        ))
    }
}
