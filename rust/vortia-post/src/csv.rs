//! Samples as comma-separated text, for a spreadsheet or a plotting script.

use std::fmt::Write as _;

use crate::case::{FieldValues, Sample};

/// A line's samples as CSV: distance, the point, the cell, then the field.
/// A point outside the mesh has an empty cell and empty values.
pub fn line_csv(field: &str, values: &FieldValues, samples: &[Sample]) -> String {
    let mut out = String::from("distance,x,y,z,cell");
    for name in values.component_names(field) {
        out.push(',');
        out.push_str(&name);
    }
    out.push('\n');
    for s in samples {
        let _ = write!(
            out,
            "{:e},{:e},{:e},{:e},",
            s.distance, s.point.x, s.point.y, s.point.z
        );
        if let Some(cell) = s.cell {
            let _ = write!(out, "{cell}");
        }
        for k in 0..values.components() {
            out.push(',');
            if let Some(v) = s.value.get(k) {
                let _ = write!(out, "{v:e}");
            }
        }
        out.push('\n');
    }
    out
}
