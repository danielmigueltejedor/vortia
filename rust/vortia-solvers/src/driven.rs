//! Boundary conditions whose values the solver sets from the flow:
//! `totalPressure`, `pressureInletVelocity` and
//! `pressureInletOutletVelocity` (`totalTemperature` and `inletOutlet` for the
//! temperature are in `thermo`).
//!
//! The formulas are those of OpenFOAM v1912 (GPL-3.0-or-later), each with
//! the density, the velocity and the mass flux the patch holds when
//! OpenFOAM updates it, which `rho_simple` arranges.

use vortia_core::Vec3;
use vortia_fields::{Data, ScalarField, VectorField};
use vortia_foamfile::Dictionary;
use vortia_fv::{Boundary, FvGeometry, ScalarBc, VectorBc, VectorBoundary};
use vortia_mesh::{Patch, PolyMesh};

use crate::error::{Result, SolverError};

/// The pressure conditions this module sets.
pub(crate) const PRESSURE_KINDS: &[&str] = &["totalPressure"];
/// The velocity conditions this module sets.
pub(crate) const VELOCITY_KINDS: &[&str] =
    &["pressureInletVelocity", "pressureInletOutletVelocity"];

/// `pos0`: 1 where `x` is not negative.
fn pos0(x: f64) -> f64 {
    if x >= 0.0 { 1.0 } else { 0.0 }
}

/// `neg`: 1 where `x` is negative.
fn neg(x: f64) -> f64 {
    1.0 - pos0(x)
}

/// Refuses an entry that renames a field the condition looks up.
fn default_names(settings: &Dictionary, what: &str, names: &[(&str, &str)]) -> Result<()> {
    for (key, default) in names {
        if settings.word(key).is_some_and(|w| w != *default) {
            return Err(SolverError::Setup(format!(
                "{what} with another {key} is not supported yet"
            )));
        }
    }
    Ok(())
}

/// The `totalPressure` patches of `p` and their `p0`.
pub(crate) struct TotalPressure {
    patches: Vec<(usize, Vec<f64>)>,
}

impl TotalPressure {
    /// Takes the patches of kind `totalPressure`. Only the form without
    /// `psi` is known: `p = p0 − ½ ρ (1 − pos0(φ)) |U|²`.
    pub(crate) fn read(file: &ScalarField, mesh: &PolyMesh) -> Result<Self> {
        let mut patches = Vec::new();
        for (i, patch) in mesh.patches().iter().enumerate() {
            let Some(pf) = file.patch_field(patch) else {
                continue;
            };
            if pf.kind != "totalPressure" {
                continue;
            }
            let what = format!("p: totalPressure on '{}'", patch.name);
            default_names(
                &pf.settings,
                &what,
                &[("U", "U"), ("phi", "phi"), ("rho", "rho"), ("psi", "none")],
            )?;
            let p0 = pf
                .settings
                .values("p0")
                .map(|v| Data::<f64>::parse(v, "p0"))
                .transpose()?
                .and_then(|d| d.expand(patch.size))
                .ok_or_else(|| {
                    SolverError::Setup(format!("{what} needs 'p0' with {} items", patch.size))
                })?;
            patches.push((i, p0));
        }
        Ok(Self { patches })
    }

    /// `totalPressure::updateCoeffs`, from the density and velocity on the
    /// boundary and the mass flux (`phi`, every face).
    pub(crate) fn update(
        &self,
        mesh: &PolyMesh,
        bc: &mut Boundary,
        rho_b: &[Option<f64>],
        u_b: &[Option<Vec3>],
        phi: &[f64],
    ) {
        let n_internal = mesh.n_internal_faces();
        for (index, p0) in &self.patches {
            let (Some(patch), Some(ScalarBc::FixedValue(values))) =
                (mesh.patches().get(*index), bc.conditions.get_mut(*index))
            else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let k = f.saturating_sub(n_internal);
                let rho = rho_b.get(k).copied().flatten().unwrap_or(0.0);
                let u = u_b.get(k).copied().flatten().unwrap_or_default();
                let flux = phi.get(f).copied().unwrap_or(0.0);
                if let (Some(v), Some(&p0)) = (values.get_mut(i), p0.get(i)) {
                    *v = p0 - 0.5 * rho * (1.0 - pos0(flux)) * u.dot(u);
                }
            }
        }
    }

    /// `p.relax()` of the boundary values these patches hold: the field is
    /// assigned `prev + α (p − prev)` everywhere, forced onto fixed values.
    pub(crate) fn relax(&self, bc: &mut Boundary, prev: &Boundary, alpha: f64) {
        for (index, _) in &self.patches {
            if let (Some(ScalarBc::FixedValue(now)), Some(ScalarBc::FixedValue(before))) =
                (bc.conditions.get_mut(*index), prev.conditions.get(*index))
            {
                for (n, &b) in now.iter_mut().zip(before) {
                    *n = b + alpha * (*n - b);
                }
            }
        }
    }
}

/// The patches whose velocity follows the pressure.
pub(crate) struct PressureVelocity {
    /// `pressureInletVelocity`: patch index.
    inlet: Vec<usize>,
    /// `pressureInletOutletVelocity`: patch index, and the `value` its file
    /// gave, which is its velocity until the first update.
    outlet: Vec<(usize, Vec<Vec3>)>,
}

impl PressureVelocity {
    pub(crate) fn read(file: &VectorField, mesh: &PolyMesh, fv: &FvGeometry) -> Result<Self> {
        let (mut inlet, mut outlet) = (Vec::new(), Vec::new());
        for (i, patch) in mesh.patches().iter().enumerate() {
            let Some(pf) = file.patch_field(patch) else {
                continue;
            };
            let what = format!("U: {} on '{}'", pf.kind, patch.name);
            match pf.kind.as_str() {
                "pressureInletVelocity" => {
                    default_names(&pf.settings, &what, &[("phi", "phi"), ("rho", "rho")])?;
                    if pf.settings.values("direction").is_some() {
                        return Err(SolverError::Setup(format!(
                            "{what} with a direction is not supported yet"
                        )));
                    }
                    inlet.push(i);
                }
                "pressureInletOutletVelocity" => {
                    default_names(&pf.settings, &what, &[("phi", "phi")])?;
                    if !along_axes(patch, fv) {
                        return Err(SolverError::Setup(format!(
                            "{what}: only a patch whose normal is along an axis is supported yet"
                        )));
                    }
                    let value = pf
                        .value
                        .as_ref()
                        .and_then(|d: &Data<Vec3>| d.expand(patch.size))
                        .ok_or_else(|| {
                            SolverError::Setup(format!(
                                "{what} needs a 'value' of {} items",
                                patch.size
                            ))
                        })?;
                    outlet.push((i, value));
                }
                _ => {}
            }
        }
        Ok(Self { inlet, outlet })
    }

    /// `updateCoeffs` of both, from the mass flux and the density on the
    /// boundary: `pressureInletVelocity` takes `n φ / (ρ |Sf|)`, and
    /// `pressureInletOutletVelocity` fixes the tangential part (zero) only
    /// where the flow comes in.
    pub(crate) fn update(
        &self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &mut VectorBoundary,
        rho_b: &[Option<f64>],
        phi: &[f64],
    ) {
        let n_internal = mesh.n_internal_faces();
        for &index in &self.inlet {
            let (Some(patch), Some(VectorBc::FixedValue(values))) =
                (mesh.patches().get(index), bc.conditions.get_mut(index))
            else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let rho = rho_b
                    .get(f.saturating_sub(n_internal))
                    .copied()
                    .flatten()
                    .unwrap_or(0.0);
                let area = fv.mag_sf.get(f).copied().unwrap_or(0.0);
                let flux = phi.get(f).copied().unwrap_or(0.0);
                let denominator = rho * area;
                let normal = unit_normal(fv, f);
                if let Some(v) = values.get_mut(i) {
                    *v = if denominator.abs() > 0.0 {
                        normal * (flux / denominator)
                    } else {
                        Vec3::default()
                    };
                }
            }
        }
        for (index, _) in &self.outlet {
            let index = *index;
            let (Some(patch), Some(VectorBc::Mixed { mask, .. })) =
                (mesh.patches().get(index), bc.conditions.get_mut(index))
            else {
                continue;
            };
            for (i, f) in patch.faces().enumerate() {
                let n = unit_normal(fv, f);
                let flux = neg(phi.get(f).copied().unwrap_or(0.0));
                if let Some(m) = mask.get_mut(i) {
                    *m = Vec3::new(
                        flux * (1.0 - n.x * n.x),
                        flux * (1.0 - n.y * n.y),
                        flux * (1.0 - n.z * n.z),
                    );
                }
            }
        }
    }
}

impl PressureVelocity {
    /// Puts the file's own `value` on the outlet patches of `u_b`, as the
    /// velocity the flux is first made from (before any update, the
    /// condition has not yet looked at the cells).
    pub(crate) fn initial_values(&self, mesh: &PolyMesh, u_b: &mut [Option<Vec3>]) {
        let n_internal = mesh.n_internal_faces();
        for (index, values) in &self.outlet {
            let Some(patch) = mesh.patches().get(*index) else {
                continue;
            };
            for (&v, f) in values.iter().zip(patch.faces()) {
                if let Some(slot) = u_b.get_mut(f.saturating_sub(n_internal)) {
                    *slot = Some(v);
                }
            }
        }
    }
}

fn unit_normal(fv: &FvGeometry, f: usize) -> Vec3 {
    let sf = fv.geo.face_areas.get(f).copied().unwrap_or_default();
    let area = fv.mag_sf.get(f).copied().unwrap_or(0.0);
    // `+ 0.0` turns a negative zero into a zero, which the files would show.
    let n = if area > 0.0 { sf * (1.0 / area) } else { sf };
    Vec3::new(n.x + 0.0, n.y + 0.0, n.z + 0.0)
}

/// Whether every face of the patch has its normal along a coordinate axis,
/// where `I − n nᵀ` is diagonal.
fn along_axes(patch: &Patch, fv: &FvGeometry) -> bool {
    patch.faces().all(|f| {
        let n = unit_normal(fv, f);
        [n.x, n.y, n.z]
            .iter()
            .all(|c| (c * c).min((1.0 - c * c).abs()) < 1.0e-9)
    })
}
