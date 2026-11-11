//! The time directories of a case: `0`, `100`, `0.005`, …

use std::path::Path;

use crate::error::Result;

/// A time directory: its value and its name exactly as on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Time {
    pub value: f64,
    pub name: String,
}

/// Every directory of `case` whose name is a number, in time order.
/// `0.orig` and other non-numeric names are not times.
pub fn times(case: impl AsRef<Path>) -> Result<Vec<Time>> {
    let case = case.as_ref();
    let entries = std::fs::read_dir(case).map_err(|source| vortia_foamfile::FoamError::Io {
        path: case.to_owned(),
        source,
    })?;
    let mut out = Vec::new();
    for entry in entries {
        // A directory that cannot be listed whole could hide the latest time.
        let entry = entry.map_err(|source| vortia_foamfile::FoamError::Io {
            path: case.to_owned(),
            source,
        })?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(value) = name.parse::<f64>().ok().filter(|v| v.is_finite()) {
            out.push(Time { value, name });
        }
    }
    out.sort_by(|a, b| a.value.total_cmp(&b.value));
    Ok(out)
}

/// The last time written, if any.
pub fn latest(case: impl AsRef<Path>) -> Result<Option<Time>> {
    Ok(times(case)?.pop())
}
