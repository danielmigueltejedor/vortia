//! Reader for the OpenFOAM case file format.
//!
//! Part of Vortia's own CFD engine. The engine reads and writes the same case
//! layout OpenFOAM does (`0/`, `constant/`, `system/`), so a case can move
//! between the two and results can be compared file for file while the engine
//! is being built.
//!
//! What is read today: the ASCII format, optionally gzip-compressed;
//! dictionaries with nested lists, dimension sets and directives (kept, not
//! evaluated); and the `polyMesh` files. Binary files are refused with an
//! error that says so.
//!
//! Every input is treated as hostile: no panics, nesting is bounded, and a
//! declared list length never decides how much memory is reserved.

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

pub mod dict;
pub mod error;
pub mod file;
pub mod lists;
pub mod macros;
pub mod polymesh;
pub mod token;
pub mod write;

pub use dict::{Dictionary, Entry, Value};
pub use error::{FoamError, Result};
pub use file::{FoamFile, Format, Header};
pub use lists::FaceList;
pub use polymesh::{RawPatch, RawPolyMesh};
pub use token::{Token, TokenKind, Tokenizer};
