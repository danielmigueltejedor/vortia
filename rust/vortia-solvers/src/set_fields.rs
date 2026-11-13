//! `setFields` for the regions the app writes: `defaultFieldValues` for every
//! cell, then `boxToCell` regions in order, each setting the cells whose
//! centre lies in its box.
//!
//! The engine's counterpart of OpenFOAM's `setFields` (OpenFOAM v1912,
//! GPL-3.0-or-later) on the start time of the case. Only the cells' values
//! change; a patch keeps the value its file gave, which no condition of the
//! solvers reads when it is not a fixed one.

use std::collections::BTreeMap;
use std::path::Path;

use vortia_core::Vec3;
use vortia_fields::{Data, ScalarField, VectorField};
use vortia_foamfile::{Dictionary, Value};

use crate::case::{Case, read_dict};
use crate::control::RunControl;
use crate::error::{Result, SolverError};

/// The most regions a `setFieldsDict` may have, and the most cell-in-box
/// tests it may cost.
const MAX_REGIONS: usize = 100_000;
const MAX_TESTS: usize = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Setting {
    Scalar(f64),
    Vector(Vec3),
}

struct Region {
    min: Vec3,
    max: Vec3,
    values: Vec<(String, Setting)>,
}

fn bad(what: impl Into<String>) -> SolverError {
    SolverError::Setup(format!("setFieldsDict: {}", what.into()))
}

fn vector(v: &Value) -> Option<Vec3> {
    let items = v.as_list()?;
    match items {
        [x, y, z] => Some(Vec3::new(x.as_scalar()?, y.as_scalar()?, z.as_scalar()?)),
        _ => None,
    }
}

/// `volScalarFieldValue p 30000 volVectorFieldValue U (0 0 0) ...`.
fn settings(items: &[Value]) -> Result<Vec<(String, Setting)>> {
    let mut out = Vec::new();
    let mut it = items.iter();
    while let Some(kind) = it.next() {
        let kind = kind
            .as_word()
            .ok_or_else(|| bad("a field value must start with its type"))?;
        let name = it
            .next()
            .and_then(Value::as_word)
            .ok_or_else(|| bad(format!("{kind} has no field name")))?;
        let value = it
            .next()
            .ok_or_else(|| bad(format!("{kind} {name} has no value")))?;
        let setting = match kind {
            "volScalarFieldValue" => value.as_scalar().map(Setting::Scalar),
            "volVectorFieldValue" => vector(value).map(Setting::Vector),
            other => return Err(bad(format!("'{other}' is not supported yet"))),
        }
        .filter(|s| match s {
            Setting::Scalar(x) => x.is_finite(),
            Setting::Vector(v) => v.x.is_finite() && v.y.is_finite() && v.z.is_finite(),
        })
        .ok_or_else(|| bad(format!("{name} has a value that is not a finite {kind}")))?;
        out.push((name.to_owned(), setting));
    }
    Ok(out)
}

fn regions(dict: &Dictionary) -> Result<Vec<Region>> {
    let Some(list) = dict.values("regions").and_then(|v| v.first()) else {
        return Ok(Vec::new());
    };
    let items = list
        .as_list()
        .ok_or_else(|| bad("regions must be a list"))?;
    if items.len() > MAX_REGIONS {
        return Err(bad(format!("{} regions", items.len())));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Keyed(kind, body) = item else {
            return Err(bad("a region is a selection with its entries in braces"));
        };
        if kind != "boxToCell" {
            return Err(bad(format!(
                "region '{kind}' is not supported yet; use boxToCell"
            )));
        }
        let corners = body.values("box").unwrap_or_default();
        let (Some(min), Some(max)) = (
            corners.first().and_then(vector),
            corners.get(1).and_then(vector),
        ) else {
            return Err(bad(
                "boxToCell needs 'box (xmin ymin zmin) (xmax ymax zmax)'",
            ));
        };
        let values = body
            .values("fieldValues")
            .and_then(|v| v.first())
            .and_then(Value::as_list)
            .ok_or_else(|| bad("boxToCell needs fieldValues"))?;
        out.push(Region {
            min,
            max,
            values: settings(values)?,
        });
    }
    Ok(out)
}

/// Sets the fields of the case's start time as `system/setFieldsDict` says
/// and writes them back. `log` receives one line per field.
pub fn run(dir: &Path, mut log: impl FnMut(&str)) -> Result<()> {
    let case = Case::open(dir)?;
    let (mesh, fv) = case.mesh()?;
    let time = RunControl::read(&case)?.start_name;
    let dict = read_dict(&dir.join("system").join("setFieldsDict"))?;
    let defaults = match dict.values("defaultFieldValues") {
        Some(v) => v
            .first()
            .and_then(Value::as_list)
            .map(settings)
            .transpose()?
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let regions = regions(&dict)?;
    let n = mesh.n_cells();
    if n.saturating_mul(regions.len()) > MAX_TESTS {
        return Err(bad(format!("{} regions in {n} cells", regions.len())));
    }

    // The fields it touches, each of one type, in the order they appear.
    let mut kinds: BTreeMap<String, bool> = BTreeMap::new();
    for (name, setting) in defaults
        .iter()
        .chain(regions.iter().flat_map(|r| &r.values))
    {
        let vector = matches!(setting, Setting::Vector(_));
        if *kinds.entry(name.clone()).or_insert(vector) != vector {
            return Err(bad(format!("{name} is set as a scalar and as a vector")));
        }
    }
    let start = dir.join(&time);
    let mut scalars: BTreeMap<String, (ScalarField, Vec<f64>)> = BTreeMap::new();
    let mut vectors: BTreeMap<String, (VectorField, Vec<Vec3>)> = BTreeMap::new();
    for (name, vector) in &kinds {
        let missing = || bad(format!("{name} does not fit the mesh"));
        if *vector {
            let field = VectorField::read(start.join(name))?;
            let cells = field.internal.expand(n).ok_or_else(missing)?;
            vectors.insert(name.clone(), (field, cells));
        } else {
            let field = ScalarField::read(start.join(name))?;
            let cells = field.internal.expand(n).ok_or_else(missing)?;
            scalars.insert(name.clone(), (field, cells));
        }
    }

    let mut set = |cell: usize, name: &str, setting: Setting| match setting {
        Setting::Scalar(x) => {
            if let Some(slot) = scalars
                .get_mut(name)
                .and_then(|(_, cells)| cells.get_mut(cell))
            {
                *slot = x;
            }
        }
        Setting::Vector(v) => {
            if let Some(slot) = vectors
                .get_mut(name)
                .and_then(|(_, cells)| cells.get_mut(cell))
            {
                *slot = v;
            }
        }
    };
    for cell in 0..n {
        for (name, setting) in &defaults {
            set(cell, name, *setting);
        }
    }
    let centres = &fv.geo.cell_centres;
    for region in &regions {
        // `boundBox::contains`: the faces of the box belong to it.
        let inside = |c: &Vec3| {
            c.x >= region.min.x
                && c.x <= region.max.x
                && c.y >= region.min.y
                && c.y <= region.max.y
                && c.z >= region.min.z
                && c.z <= region.max.z
        };
        for (cell, c) in centres.iter().enumerate() {
            if inside(c) {
                for (name, setting) in &region.values {
                    set(cell, name, *setting);
                }
            }
        }
    }

    for (name, (mut field, cells)) in scalars {
        field.internal = Data::NonUniform(cells);
        field.write(dir, &time)?;
        log(&format!("Setting {name}\n"));
    }
    for (name, (mut field, cells)) in vectors {
        field.internal = Data::NonUniform(cells);
        field.write(dir, &time)?;
        log(&format!("Setting {name}\n"));
    }
    log(&format!(
        "Set {} fields in {} regions\n",
        kinds.len(),
        regions.len()
    ));
    Ok(())
}
