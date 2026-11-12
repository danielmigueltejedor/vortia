//! The polyhedral finite-volume mesh of Vortia's own CFD engine.
//!
//! - [`PolyMesh`]: topology in OpenFOAM's face-addressed layout, read from a
//!   case's `constant/polyMesh` or built in memory, and validated so nothing
//!   downstream can index out of range.
//! - [`MeshGeometry`]: face centres and area vectors, cell centres and volumes.
//! - [`check`]: the quality report `checkMesh` gives, from that geometry.
//! - [`blocks`]: structured hexahedral boxes, the start of the engine's mesher.

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

pub mod blockmesh;
pub mod blocks;
pub mod check;
pub mod error;
pub mod geometry;
pub mod gmsh;
pub mod polymesh;
pub mod wall;

pub use check::{MeshReport, check};
pub use error::{MeshError, Result};
pub use geometry::MeshGeometry;
pub use polymesh::{Patch, PatchKind, PolyMesh};
