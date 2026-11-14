//! Looking into results: probes, line sampling and integrals over patches
//! and the volume, read from a case on disk.
//!
//! First slice of the post-processing the simulation roadmap
//! (`docs/architecture/simulation-depth.md`, P1) puts ahead of every new
//! solver. The definitions follow OpenFOAM's `probes`, `sets` (cell
//! interpolation), `patchAverage`, `patchIntegrate` and `volFieldValue`
//! function objects, so the numbers can be compared with `postProcess` on the
//! same files.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable,
    clippy::indexing_slicing
)]

mod case;
mod csv;
mod error;
mod stats;
mod wall;

pub use case::{Case, FieldReport, FieldValues, MAX_LINE_POINTS, PatchReport, Sample};
pub use csv::line_csv;
pub use error::{PostError, Result};
pub use stats::{PatchStats, VolumeStats};
pub use wall::WallReport;

#[cfg(test)]
mod tests;
