//! Solvers of Vortia's CFD engine, run on a case directory.
//!
//! A solver reads the same case OpenFOAM's counterpart reads and writes the
//! same files, so each can be checked against OpenFOAM on one case.

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

pub mod case;
pub mod error;
pub mod laplacian;

pub use case::Case;
pub use error::{Result, SolverError};

/// Runs the solver the case's `controlDict` names in `application`.
pub fn run(case: &Case, log: impl FnMut(&str)) -> Result<()> {
    match case.application() {
        Some("laplacianFoam") => laplacian::run(case, log).map(|_| ()),
        Some(other) => Err(SolverError::Setup(format!(
            "the engine has no counterpart of '{other}' yet"
        ))),
        None => Err(SolverError::Setup(
            "controlDict names no 'application'".into(),
        )),
    }
}
