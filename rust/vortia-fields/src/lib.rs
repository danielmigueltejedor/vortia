//! Fields of Vortia's CFD engine: values on cells or faces, with a boundary
//! condition per patch, read and written in the case format.
//!
//! Stage E2 of the engine (see `docs/architecture/rust-engine.md`): a field
//! read from a case, checked against its mesh and written back is the same
//! field, and OpenFOAM reads what is written. Conditions are kept by name with
//! all their settings; evaluating them comes with the operators (stage E3).

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

pub mod data;
pub mod error;
pub mod field;
pub mod time;
pub mod value;

pub use data::Data;
pub use error::{FieldError, Result};
pub use field::{Field, Location, PatchField};
pub use time::{Time, latest, times};
pub use value::FieldValue;

/// A field of numbers: pressure, temperature, a flux.
pub type ScalarField = Field<f64>;
/// A field of vectors: velocity.
pub type VectorField = Field<vortia_core::Vec3>;
