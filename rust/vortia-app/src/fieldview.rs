//! A field of a case on the cells of its mesh, for the viewer to colour the surface with: read
//! from a time directory as one number per cell (a vector gives its length), with the range of
//! the numbers, and a colour scale to turn them into colours.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use vortia_core::Vec3;
use vortia_fields::{Field, Location};

/// A field as one number per cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellValues {
    pub name: String,
    pub time: String,
    /// `scalar` or `vector` (then the numbers are lengths).
    pub kind: &'static str,
    pub values: Vec<f32>,
    /// The smallest and largest of the finite values; `None` when there are none.
    pub range: Option<(f32, f32)>,
}

impl CellValues {
    fn new(name: &str, time: &str, kind: &'static str, values: Vec<f32>) -> Self {
        let mut range: Option<(f32, f32)> = None;
        for &v in values.iter().filter(|v| v.is_finite()) {
            range = Some(match range {
                Some((lo, hi)) => (lo.min(v), hi.max(v)),
                None => (v, v),
            });
        }
        Self {
            name: name.to_owned(),
            time: time.to_owned(),
            kind,
            values,
            range,
        }
    }

    /// Where the value of a cell falls in the range, 0 to 1; `None` for a cell with no finite
    /// value.
    pub fn fraction(&self, cell: usize) -> Option<f32> {
        let value = *self.values.get(cell)?;
        let (lo, hi) = self.range?;
        if !value.is_finite() {
            return None;
        }
        Some(if hi > lo {
            ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
        } else {
            0.5
        })
    }
}

/// Reads `<case>/<time>/<name>` for a mesh of `cells` cells.
pub fn read(case: &Path, time: &str, name: &str, cells: usize) -> Result<CellValues, String> {
    // The names come from folder listings, but a hostile folder can hold anything.
    let plain =
        |part: &str| !part.is_empty() && !part.contains(['/', '\\']) && part != ".." && part != ".";
    if !plain(time) || !plain(name) {
        return Err(format!("'{time}/{name}' is not a field of the case."));
    }
    let path = case.join(time).join(name);
    let too_few = |n: usize| format!("{name} has {n} values, but the mesh has {cells} cells.");
    let faces = || format!("{name} has a value on each face, not on each cell.");
    match Field::<f64>::read(&path) {
        Ok(field) if field.location != Location::Cells => Err(faces()),
        Ok(field) => {
            let values = field
                .internal
                .expand(cells)
                .ok_or_else(|| too_few(field.internal.count().unwrap_or(0)))?;
            Ok(CellValues::new(
                name,
                time,
                "scalar",
                values.into_iter().map(|v| v as f32).collect(),
            ))
        }
        Err(scalar_error) => match Field::<Vec3>::read(&path) {
            Ok(field) if field.location != Location::Cells => Err(faces()),
            Ok(field) => {
                let values = field
                    .internal
                    .expand(cells)
                    .ok_or_else(|| too_few(field.internal.count().unwrap_or(0)))?;
                Ok(CellValues::new(
                    name,
                    time,
                    "vector",
                    values
                        .into_iter()
                        .map(|v| (v.x * v.x + v.y * v.y + v.z * v.z).sqrt() as f32)
                        .collect(),
                ))
            }
            // The scalar reading's error says more when the file is not a vector either.
            Err(_) => Err(format!(
                "{name} cannot be shown on the mesh: {scalar_error}"
            )),
        },
    }
}

/// Reads a field on a thread; `wake` is called when it is ready.
pub fn spawn(
    case: PathBuf,
    time: String,
    name: String,
    cells: usize,
    wake: impl Fn() + Send + 'static,
) -> Receiver<Result<CellValues, String>> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("vortia-field".into())
        .spawn(move || {
            let read = std::panic::catch_unwind(|| read(&case, &time, &name, cells))
                .unwrap_or_else(|_| Err("The field could not be read.".to_owned()));
            let _ = tx.send(read);
            wake();
        });
    // No thread, no field: the viewer keeps the colours of the patches.
    drop(spawned);
    rx
}

/// The colour scale, from low to high: deep blue, blue, teal, green, yellow, orange, red.
const STOPS: [[u8; 3]; 7] = [
    [0x26, 0x2a, 0x7a],
    [0x2c, 0x6b, 0xc9],
    [0x22, 0xa7, 0xa8],
    [0x5f, 0xc2, 0x5d],
    [0xe6, 0xd8, 0x3c],
    [0xf2, 0x8e, 0x2b],
    [0xc9, 0x2e, 0x2e],
];

/// The colour for a place on the scale, 0 to 1; a place outside it is the nearest end.
pub fn scale(t: f32) -> [u8; 3] {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let position = t * (STOPS.len() - 1) as f32;
    let index = (position.floor() as usize).min(STOPS.len() - 2);
    let along = position - index as f32;
    let (Some(a), Some(b)) = (STOPS.get(index), STOPS.get(index + 1)) else {
        return [0; 3];
    };
    let mix = |i: usize| -> u8 {
        let (x, y) = (
            f32::from(a.get(i).copied().unwrap_or(0)),
            f32::from(b.get(i).copied().unwrap_or(0)),
        );
        (x + (y - x) * along).round().clamp(0.0, 255.0) as u8
    };
    [mix(0), mix(1), mix(2)]
}

/// A number for a label of the scale: few digits, scientific when it is very small or large.
pub fn label(value: f32) -> String {
    let magnitude = value.abs();
    if value == 0.0 || (1e-3..1e5).contains(&magnitude) {
        let text = format!("{value:.4}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        text.to_owned()
    } else {
        format!("{value:.3e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, time: &str, name: &str, class: &str, internal: &str) {
        let path = dir.join(time).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "FoamFile {{ version 2.0; format ascii; class {class}; object {name}; }}\n\
                 dimensions [0 0 0 0 0 0 0];\ninternalField {internal};\n\
                 boundaryField {{ wall {{ type zeroGradient; }} }}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn a_scalar_field_gives_its_values_and_range() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "0.5",
            "p",
            "volScalarField",
            "nonuniform List<scalar> 3(1 5 3)",
        );
        let field = read(dir.path(), "0.5", "p", 3).unwrap();
        assert_eq!(field.kind, "scalar");
        assert_eq!(field.values, [1.0, 5.0, 3.0]);
        assert_eq!(field.range, Some((1.0, 5.0)));
        assert_eq!(field.fraction(0), Some(0.0));
        assert_eq!(field.fraction(1), Some(1.0));
        assert_eq!(field.fraction(2), Some(0.5));
        assert_eq!(field.fraction(9), None);
    }

    #[test]
    fn a_uniform_field_fills_every_cell_and_sits_in_the_middle_of_the_scale() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "0", "T", "volScalarField", "uniform 300");
        let field = read(dir.path(), "0", "T", 4).unwrap();
        assert_eq!(field.values, [300.0; 4]);
        assert_eq!(field.fraction(2), Some(0.5));
    }

    #[test]
    fn a_vector_field_is_shown_by_its_length() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "0",
            "U",
            "volVectorField",
            "nonuniform List<vector> 2((3 4 0)(0 0 2))",
        );
        let field = read(dir.path(), "0", "U", 2).unwrap();
        assert_eq!(field.kind, "vector");
        assert_eq!(field.values, [5.0, 2.0]);
    }

    #[test]
    fn a_field_of_another_size_or_place_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "0",
            "p",
            "volScalarField",
            "nonuniform List<scalar> 2(1 2)",
        );
        write(dir.path(), "0", "phi", "surfaceScalarField", "uniform 0");
        assert!(
            read(dir.path(), "0", "p", 3)
                .unwrap_err()
                .contains("2 values")
        );
        assert!(read(dir.path(), "0", "phi", 3).is_err());
        assert!(read(dir.path(), "0", "missing", 3).is_err());
        for hostile in ["../p", "a/b", "..", ""] {
            assert!(read(dir.path(), "0", hostile, 2).is_err());
            assert!(read(dir.path(), hostile, "p", 2).is_err());
        }
    }

    #[test]
    fn numbers_that_are_not_finite_have_no_colour_and_do_not_set_the_range() {
        let field = CellValues::new("x", "0", "scalar", vec![f32::NAN, 2.0, f32::INFINITY, 4.0]);
        assert_eq!(field.range, Some((2.0, 4.0)));
        assert_eq!(field.fraction(0), None);
        assert_eq!(field.fraction(2), None);
        assert_eq!(
            CellValues::new("x", "0", "scalar", vec![f32::NAN]).range,
            None
        );
    }

    #[test]
    fn the_scale_runs_from_the_first_stop_to_the_last_and_never_leaves_them() {
        assert_eq!(scale(0.0), STOPS[0]);
        assert_eq!(scale(1.0), STOPS[6]);
        assert_eq!(scale(-5.0), STOPS[0]);
        assert_eq!(scale(5.0), STOPS[6]);
        assert_eq!(scale(f32::NAN), STOPS[0]);
        assert_eq!(scale(0.5), STOPS[3]);
    }

    #[test]
    fn labels_are_short() {
        assert_eq!(label(0.0), "0");
        assert_eq!(label(101325.0), "1.013e5");
        assert_eq!(label(300.5), "300.5");
        assert_eq!(label(-0.25), "-0.25");
        assert_eq!(label(2.5e-6), "2.500e-6");
    }
}
