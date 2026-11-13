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
    /// Patch index, `p0`, and `γ` for the form that takes the compressibility
    /// `ψ`, high-speed flow's.
    patches: Vec<(usize, Vec<f64>, Option<f64>)>,
}

impl TotalPressure {
    /// Takes the patches of kind `totalPressure`: without `psi`,
    /// `p = p0 − ½ ρ (1 − pos0(φ)) |U|²`; with `psi thermo:psi` and a `gamma`,
    /// the high-speed form `p0 / (1 + ½ ψ (γ−1)/γ (1 − pos0(φ)) |U|²)^(γ/(γ−1))`.
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
                &[("U", "U"), ("phi", "phi"), ("rho", "rho")],
            )?;
            let gamma = match pf.settings.word("psi") {
                None | Some("none") => None,
                Some("thermo:psi") => Some(pf.settings.scalar("gamma").ok_or_else(|| {
                    SolverError::Setup(format!("{what} with psi needs a 'gamma'"))
                })?),
                Some(other) => {
                    return Err(SolverError::Setup(format!(
                        "{what} with psi '{other}' is not supported yet; use thermo:psi"
                    )));
                }
            };
            let p0 = pf
                .settings
                .values("p0")
                .map(|v| Data::<f64>::parse(v, "p0"))
                .transpose()?
                .and_then(|d| d.expand(patch.size))
                .ok_or_else(|| {
                    SolverError::Setup(format!("{what} needs 'p0' with {} items", patch.size))
                })?;
            patches.push((i, p0, gamma));
        }
        Ok(Self { patches })
    }

    /// Whether a patch takes the compressibility `psi` and `gamma`, which the
    /// solvers that carry no `psi` on the boundary cannot give it.
    pub(crate) fn uses_psi(&self) -> bool {
        self.patches.iter().any(|(_, _, gamma)| gamma.is_some())
    }

    /// `totalPressure::updateCoeffs`, from the density or the
    /// compressibility `psi_b` and the velocity on the boundary and the mass
    /// flux (`phi`, every face).
    pub(crate) fn update(
        &self,
        mesh: &PolyMesh,
        bc: &mut Boundary,
        (rho_b, psi_b): (&[Option<f64>], &[Option<f64>]),
        u_b: &[Option<Vec3>],
        phi: &[f64],
    ) {
        let n_internal = mesh.n_internal_faces();
        for (index, p0, gamma) in &self.patches {
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
                let psi = psi_b.get(k).copied().flatten().unwrap_or(0.0);
                if let (Some(v), Some(&p0)) = (values.get_mut(i), p0.get(i)) {
                    *v = match gamma {
                        None => p0 - 0.5 * rho * (1.0 - pos0(flux)) * u.dot(u),
                        Some(g) if *g > 1.0 => {
                            let g_m1_by_g = (g - 1.0) / g;
                            p0 / (1.0 + 0.5 * psi * g_m1_by_g * (1.0 - pos0(flux)) * u.dot(u))
                                .powf(1.0 / g_m1_by_g)
                        }
                        Some(_) => p0 / (1.0 + 0.5 * psi * (1.0 - pos0(flux)) * u.dot(u)),
                    };
                }
            }
        }
    }

    /// `p.relax()` of the boundary values these patches hold: the field is
    /// assigned `prev + α (p − prev)` everywhere, forced onto fixed values.
    pub(crate) fn relax(&self, bc: &mut Boundary, prev: &Boundary, alpha: f64) {
        for (index, _, _) in &self.patches {
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
    /// The conditions as they stand before the first update, for what looks
    /// at the velocity before then (the turbulence model validating its
    /// viscosity): an outlet holds the `value` its file gave, not what its
    /// condition would take from the cells, and, with no value fraction yet,
    /// a normal gradient of zero. The second result lists those patches.
    pub(crate) fn initial_boundary(&self, bc: &VectorBoundary) -> (VectorBoundary, Vec<usize>) {
        let mut out = bc.clone();
        let mut patches = Vec::new();
        for (index, values) in &self.outlet {
            if let Some(slot) = out.conditions.get_mut(*index) {
                *slot = VectorBc::FixedValue(values.clone());
                patches.push(*index);
            }
        }
        (out, patches)
    }

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

#[cfg(test)]
mod tests {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box;

    use super::*;

    /// One cell of volume 1; the six patches are xMin, xMax, yMin, yMax,
    /// zMin and zMax, of one face each.
    fn cell() -> (PolyMesh, FvGeometry) {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let mesh = structured_box(bounds, [1, 1, 1]).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    /// Flux on the faces: the six boundary faces come after the internal
    /// ones, in patch order.
    fn flux(mesh: &PolyMesh, outward: f64) -> Vec<f64> {
        vec![outward; mesh.n_faces()]
    }

    #[test]
    fn total_pressure_loses_the_dynamic_pressure_only_where_the_flow_comes_in() {
        let (mesh, _) = cell();
        let total = TotalPressure {
            patches: vec![(0, vec![1000.0], None), (1, vec![1000.0], None)],
        };
        let mut bc = Boundary {
            conditions: vec![ScalarBc::FixedValue(vec![0.0]); 6],
        };
        let rho_b = vec![Some(2.0); 6];
        let u_b = vec![Some(Vec3::new(3.0, 0.0, 4.0)); 6];
        // Flux into the domain: p0 − ½ ρ |U|² = 1000 − 25.
        total.update(&mesh, &mut bc, (&rho_b, &[]), &u_b, &flux(&mesh, -1.0));
        assert_eq!(bc.conditions[0], ScalarBc::FixedValue(vec![975.0]));
        // Flux out of it, and none: p0.
        total.update(&mesh, &mut bc, (&rho_b, &[]), &u_b, &flux(&mesh, 1.0));
        assert_eq!(bc.conditions[1], ScalarBc::FixedValue(vec![1000.0]));
        total.update(&mesh, &mut bc, (&rho_b, &[]), &u_b, &flux(&mesh, 0.0));
        assert_eq!(bc.conditions[0], ScalarBc::FixedValue(vec![1000.0]));
        // The patches it does not own stay as they were.
        assert_eq!(bc.conditions[2], ScalarBc::FixedValue(vec![0.0]));
    }

    #[test]
    fn total_pressure_with_psi_follows_the_isentropic_relation() {
        let (mesh, _) = cell();
        let gamma = 1.4;
        let total = TotalPressure {
            patches: vec![(0, vec![1.0e5], Some(gamma))],
        };
        let mut bc = Boundary {
            conditions: vec![ScalarBc::FixedValue(vec![0.0]); 6],
        };
        let psi_b = vec![Some(1.0e-5); 6];
        let u_b = vec![Some(Vec3::new(60.0, 0.0, 80.0)); 6];
        let (rho_b, k) = (vec![None; 6], (gamma - 1.0) / gamma);
        total.update(&mesh, &mut bc, (&rho_b, &psi_b), &u_b, &flux(&mesh, -1.0));
        let want = 1.0e5 / (1.0 + 0.5 * 1.0e-5 * k * 1.0e4).powf(1.0 / k);
        let ScalarBc::FixedValue(got) = &bc.conditions[0] else {
            panic!("not fixed");
        };
        assert!((got[0] - want).abs() < 1e-9 * want, "{} vs {want}", got[0]);
        // Flow out of the domain: the total pressure itself.
        total.update(&mesh, &mut bc, (&rho_b, &psi_b), &u_b, &flux(&mesh, 1.0));
        assert_eq!(bc.conditions[0], ScalarBc::FixedValue(vec![1.0e5]));
    }

    #[test]
    fn total_pressure_relaxes_towards_what_it_held() {
        let total = TotalPressure {
            patches: vec![(0, vec![0.0], None)],
        };
        let before = Boundary {
            conditions: vec![ScalarBc::FixedValue(vec![100.0]), ScalarBc::ZeroGradient],
        };
        let mut now = Boundary {
            conditions: vec![ScalarBc::FixedValue(vec![200.0]), ScalarBc::ZeroGradient],
        };
        total.relax(&mut now, &before, 0.3);
        assert_eq!(now.conditions[0], ScalarBc::FixedValue(vec![130.0]));
    }

    #[test]
    fn the_outlet_fixes_the_tangential_velocity_only_when_the_flow_reverses() {
        let (mesh, fv) = cell();
        let driven = PressureVelocity {
            inlet: Vec::new(),
            outlet: vec![(1, vec![Vec3::default()])],
        };
        let mut conditions = vec![VectorBc::ZeroGradient; 6];
        conditions[1] = VectorBc::Mixed {
            mask: vec![Vec3::default()],
            ref_value: vec![Vec3::default()],
        };
        let mut bc = VectorBoundary {
            assignable: vec![false; 6],
            conditions,
        };
        let rho_b = vec![Some(1.0); 6];
        // Flow in through xMax: the y and z components are held at zero,
        // the x component stays free.
        driven.update(&mesh, &fv, &mut bc, &rho_b, &flux(&mesh, -1.0));
        let u = Vec3::new(5.0, 6.0, 7.0);
        assert_eq!(
            bc.face_values(&mesh, &[u])[1],
            Some(Vec3::new(5.0, 0.0, 0.0))
        );
        // Flow out: a zero gradient.
        driven.update(&mesh, &fv, &mut bc, &rho_b, &flux(&mesh, 1.0));
        assert_eq!(bc.face_values(&mesh, &[u])[1], Some(u));
    }

    #[test]
    fn the_inlet_velocity_is_the_flux_over_the_mass_through_the_face() {
        let (mesh, fv) = cell();
        let driven = PressureVelocity {
            inlet: vec![0],
            outlet: Vec::new(),
        };
        let mut bc = VectorBoundary {
            assignable: vec![true, false, false, false, false, false],
            conditions: vec![VectorBc::FixedValue(vec![Vec3::default()]); 6],
        };
        // A mass flux of −3 through a face of area 1 at a density of 2
        // enters along −n = +x at 1.5.
        driven.update(&mesh, &fv, &mut bc, &[Some(2.0); 6], &flux(&mesh, -3.0));
        assert_eq!(
            bc.conditions[0],
            VectorBc::FixedValue(vec![Vec3::new(1.5, 0.0, 0.0)])
        );
        // No density: no velocity, rather than a division by zero.
        driven.update(&mesh, &fv, &mut bc, &[Some(0.0); 6], &flux(&mesh, -3.0));
        assert_eq!(
            bc.conditions[0],
            VectorBc::FixedValue(vec![Vec3::default()])
        );
    }
}
