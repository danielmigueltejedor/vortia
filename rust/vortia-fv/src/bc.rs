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
        check_solvable(field)?;
        field.check_against(mesh)?;
        let mut conditions = Vec::with_capacity(mesh.patches().len());
        for patch in mesh.patches() {
            let pf = field.patch_field(patch).ok_or_else(|| {
                FvError::Condition(format!("no condition for patch '{}'", patch.name))
            })?;
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
