//! Boundary conditions of scalar fields, evaluated.
//!
//! Each condition answers two questions: what is the value on its faces given
//! the cells next to them, and what does it add to an implicit Laplacian. The
//! answers follow OpenFOAM's `fixedValue`, `zeroGradient`, `fixedGradient`,
//! `empty`, `wedge` and `symmetry` conditions (OpenFOAM, GPL-3.0-or-later).
//! For a scalar, `wedge` and `symmetry` reflect nothing: the face takes the
//! cell's value and carries no flux.

use vortia_fields::{Data, Field, FieldValue, Location, ScalarField};
use vortia_mesh::{Patch, PatchKind, PolyMesh};

use crate::error::{FvError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum ScalarBc {
    FixedValue(Vec<f64>),
    ZeroGradient,
    FixedGradient(Vec<f64>),
    /// The unused direction of a 2D case: the faces are not part of the
    /// finite-volume mesh.
    Empty,
    /// Wedge and symmetry sides: zero gradient for a scalar.
    Reflect,
    /// Values computed elsewhere; cannot be used in an implicit equation.
    Calculated(Vec<f64>),
    /// OpenFOAM's `mixed` with a zero reference gradient: per face, the
    /// fraction `f` takes `ref_value` and the rest a zero gradient. What
    /// `inletOutlet` is, with `f = 1` where the flow comes in.
    Mixed {
        fraction: Vec<f64>,
        ref_value: Vec<f64>,
    },
}

/// One condition per mesh patch, in patch order.
#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub conditions: Vec<ScalarBc>,
}

fn values(data: Option<&Data<f64>>, patch: &Patch, what: &str) -> Result<Vec<f64>> {
    data.and_then(|d| d.expand(patch.size)).ok_or_else(|| {
        FvError::Condition(format!(
            "patch '{}': {what} needs a 'value' of {} items",
            patch.name, patch.size
        ))
    })
}

impl Boundary {
    pub fn from_field(field: &ScalarField, mesh: &PolyMesh) -> Result<Self> {
        Self::from_field_with(field, mesh, &[])
    }

    /// As [`from_field`](Self::from_field), also taking the conditions named
    /// in `driven`, whose values the solver sets from the flow:
    /// `totalPressure` and `totalTemperature` start as their file's `value`,
    /// `inletOutlet` as a zero gradient towards its `inletValue`, and
    /// `fixedFluxPressure` as a zero gradient.
    pub fn from_field_with(field: &ScalarField, mesh: &PolyMesh, driven: &[&str]) -> Result<Self> {
        check_solvable(field)?;
        field.check_against(mesh)?;
        let mut conditions = Vec::with_capacity(mesh.patches().len());
        for patch in mesh.patches() {
            let pf = field.patch_field(patch).ok_or_else(|| {
                FvError::Condition(format!("no condition for patch '{}'", patch.name))
            })?;
            if (patch.kind == PatchKind::Wedge) != (pf.kind == "wedge") {
                return Err(FvError::Condition(format!(
                    "patch '{}' is a {} patch but the field says {}",
                    patch.name,
                    patch.kind.name(),
                    pf.kind
                )));
            }
            let bc = match pf.kind.as_str() {
                "fixedValue" => {
                    ScalarBc::FixedValue(values(pf.value.as_ref(), patch, "fixedValue")?)
                }
                "zeroGradient" => ScalarBc::ZeroGradient,
                "fixedGradient" => {
                    let g = pf
                        .settings
                        .values("gradient")
                        .map(|v| Data::<f64>::parse(v, "gradient"))
                        .transpose()?;
                    ScalarBc::FixedGradient(values(g.as_ref(), patch, "fixedGradient's gradient")?)
                }
                "empty" => ScalarBc::Empty,
                "totalPressure" | "totalTemperature" if driven.contains(&pf.kind.as_str()) => {
                    // Without a `value`, OpenFOAM starts from p0 (or T0).
                    let total = match pf.kind.as_str() {
                        "totalPressure" => "p0",
                        _ => "T0",
                    };
                    let start = match pf.value.as_ref() {
                        Some(_) => None,
                        None => pf
                            .settings
                            .values(total)
                            .map(|v| Data::<f64>::parse(v, total))
                            .transpose()?,
                    };
                    ScalarBc::FixedValue(values(
                        pf.value.as_ref().or(start.as_ref()),
                        patch,
                        &pf.kind,
                    )?)
                }
                "codedMixed" if driven.contains(&"codedMixed") => {
                    let read = |key: &str| {
                        pf.settings
                            .values(key)
                            .map(|v| Data::<f64>::parse(v, key))
                            .transpose()
                    };
                    if let Some(g) = read("refGradient")?
                        && g.expand(patch.size)
                            .is_none_or(|g| g.iter().any(|&g| g != 0.0))
                    {
                        return Err(FvError::Condition(format!(
                            "patch '{}': codedMixed with a reference gradient is not supported yet",
                            patch.name
                        )));
                    }
                    ScalarBc::Mixed {
                        fraction: values(read("valueFraction")?.as_ref(), patch, "valueFraction")?,
                        ref_value: values(read("refValue")?.as_ref(), patch, "refValue")?,
                    }
                }
                "inletOutlet" if driven.contains(&"inletOutlet") => {
                    let given = pf
                        .settings
                        .values("inletValue")
                        .map(|v| Data::<f64>::parse(v, "inletValue"))
                        .transpose()?;
                    ScalarBc::Mixed {
                        fraction: vec![0.0; patch.size],
                        ref_value: values(given.as_ref(), patch, "inletOutlet's inletValue")?,
                    }
                }
                // A pressure whose gradient makes the flux through the face
                // the one the velocity condition asks for; the solver sets
                // it, from zero.
                "fixedFluxPressure" if driven.contains(&"fixedFluxPressure") => {
                    ScalarBc::FixedGradient(vec![0.0; patch.size])
                }
                "wedge" | "symmetry" | "symmetryPlane" => ScalarBc::Reflect,
                "calculated" => {
                    ScalarBc::Calculated(values(pf.value.as_ref(), patch, "calculated")?)
                }
                other => {
                    return Err(FvError::Condition(format!(
                        "patch '{}': '{other}' is not supported yet",
                        patch.name
                    )));
                }
            };
            let expected_kind = match bc {
                ScalarBc::Empty => Some(PatchKind::Empty),
                _ => None,
            };
            if let Some(k) = expected_kind
                && patch.kind != k
            {
                return Err(FvError::Condition(format!(
                    "patch '{}' is a {} patch but the field says empty",
                    patch.name,
                    patch.kind.name()
                )));
            }
            if patch.kind == PatchKind::Empty && bc != ScalarBc::Empty {
                return Err(FvError::Condition(format!(
                    "patch '{}' is empty, so its condition must be 'empty'",
                    patch.name
                )));
            }
            conditions.push(bc);
        }
        Ok(Self { conditions })
    }

    /// The value on every boundary face, given the cell values and each
    /// face's delta coefficient (for `fixedGradient`). `None` for faces of empty patches.
    pub fn face_values(
        &self,
        mesh: &PolyMesh,
        cells: &[f64],
        delta_coeffs: &[f64],
    ) -> Vec<Option<f64>> {
        let mut out = Vec::with_capacity(mesh.n_faces().saturating_sub(mesh.n_internal_faces()));
        for (patch, bc) in mesh.patches().iter().zip(&self.conditions) {
            for (i, f) in patch.faces().enumerate() {
                let inside = mesh
                    .owner()
                    .get(f)
                    .and_then(|&c| cells.get(c))
                    .copied()
                    .unwrap_or(0.0);
                let v = match bc {
                    ScalarBc::FixedValue(v) | ScalarBc::Calculated(v) => v.get(i).copied(),
                    ScalarBc::ZeroGradient | ScalarBc::Reflect => Some(inside),
                    ScalarBc::Mixed {
                        fraction,
                        ref_value,
                    } => {
                        let f = fraction.get(i).copied().unwrap_or(0.0);
                        let r = ref_value.get(i).copied().unwrap_or(0.0);
                        Some(f * r + (1.0 - f) * inside)
                    }
                    ScalarBc::FixedGradient(g) => {
                        let dc = delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let g = g.get(i).copied().unwrap_or(0.0);
                        Some(if dc > 0.0 { inside + g / dc } else { inside })
                    }
                    ScalarBc::Empty => None,
                };
                out.push(v);
            }
        }
        out
    }
}

/// Refuses a field a solver cannot take as it stands: one on faces, or one
/// whose conditions depend on a directive the reader keeps unevaluated.
pub fn check_solvable<T: FieldValue>(field: &Field<T>) -> Result<()> {
    if field.location != Location::Cells {
        return Err(FvError::Condition(format!(
            "{} is not a cell field",
            field.name
        )));
    }
    if let Some(d) = field.unevaluated_directive() {
        return Err(FvError::Condition(format!(
            "{}: '{d}' is not evaluated yet; write its entries out in the field",
            field.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use vortia_core::{Aabb, Vec3};
    use vortia_mesh::blocks::structured_box;

    use super::*;
    use crate::vector::{VectorBc, VectorBoundary};

    /// One cell of volume 1: six patches of one face each.
    fn cell() -> PolyMesh {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        structured_box(bounds, [1, 1, 1]).unwrap()
    }

    #[test]
    fn a_mixed_value_is_the_reference_where_the_fraction_is_one_and_the_cell_where_it_is_zero() {
        let mesh = cell();
        let mixed = |fraction: f64| ScalarBc::Mixed {
            fraction: vec![fraction],
            ref_value: vec![10.0],
        };
        let bc = Boundary {
            conditions: vec![
                mixed(1.0),
                mixed(0.0),
                mixed(0.25),
                ScalarBc::ZeroGradient,
                ScalarBc::ZeroGradient,
                ScalarBc::ZeroGradient,
            ],
        };
        let faces = bc.face_values(&mesh, &[2.0], &[1.0; 6]);
        assert_eq!(faces[..3], [Some(10.0), Some(2.0), Some(4.0)]);
    }

    #[test]
    fn a_mixed_velocity_takes_the_reference_only_on_the_masked_components() {
        let mesh = cell();
        let mut conditions = vec![VectorBc::ZeroGradient; 6];
        conditions[0] = VectorBc::Mixed {
            mask: vec![Vec3::new(0.0, 1.0, 1.0)],
            ref_value: vec![Vec3::new(9.0, 8.0, 7.0)],
        };
        let bc = VectorBoundary {
            assignable: vec![false; 6],
            conditions,
        };
        let faces = bc.face_values(&mesh, &[Vec3::new(1.0, 2.0, 3.0)]);
        assert_eq!(faces[0], Some(Vec3::new(1.0, 8.0, 7.0)));
        assert_eq!(faces[1], Some(Vec3::new(1.0, 2.0, 3.0)));
        // A mixed condition does not fix the velocity: a correction may
        // still move it.
        assert!(!bc.fixes_value(0));
    }

    #[test]
    fn a_velocity_that_follows_the_pressure_is_assignable_though_fixed() {
        let mesh = cell();
        let bc = VectorBoundary {
            conditions: vec![VectorBc::FixedValue(vec![Vec3::default()]); mesh.patches().len()],
            assignable: vec![true, false, false, false, false, false],
        };
        assert!(!bc.fixes_value(0));
        assert!(bc.fixes_value(1));
    }
}
