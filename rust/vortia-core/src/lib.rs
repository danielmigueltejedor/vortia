//! Vortia core — Rust port (in progress).
//!
//! Mirrors `src/core` of the C++ codebase. The port is incremental: the C++
//! application remains the primary implementation. The unit tests are the same
//! cases as `tests/unit/test_{units,version,project}.cpp`.

pub mod error;
pub mod process;
pub mod project;
pub mod units;
pub mod vec3;
pub mod version;

pub use error::{ErrorCode, Result, VortiaError};
pub use project::{CaseState, Project};
pub use units::{KinematicViscosity, Length, Pressure, Time, Velocity};
pub use vec3::{Aabb, Vec3};
pub use version::Version;
