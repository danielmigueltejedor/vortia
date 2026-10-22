//! Vortia core — Rust port (in progress).
//!
//! Mirrors `src/core` of the C++ codebase. The port is incremental: the C++
//! application remains the primary implementation for the 2026-10 practice.

pub mod error;
pub mod process;
pub mod project;
pub mod units;

pub use error::{ErrorCode, VortiaError};
pub use project::Project;
pub use units::{Dimensionless, KinematicViscosity, Length, Pressure, Quantity, Time, Velocity};