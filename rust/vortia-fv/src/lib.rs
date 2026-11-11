//! Finite-volume discretisation and linear solvers of Vortia's CFD engine.
//!
//! - [`LduMatrix`]: sparse matrices addressed by the mesh's faces.
//! - [`solve`]: iterative solvers with OpenFOAM's residual definition.
//! - [`FvGeometry`]: interpolation weights and face-gradient coefficients.
//! - [`bc`]: scalar boundary conditions, evaluated.
//! - [`ops`]: gradient, diffusion, convection and time-derivative operators.
//! - [`vector`]: vector conditions and equations (momentum).

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

pub mod bc;
pub mod error;
pub mod fvgeom;
pub mod ldu;
pub mod ops;
pub mod solve;
pub mod vector;

pub use bc::{Boundary, ScalarBc};
pub use error::{FvError, Result};
pub use fvgeom::FvGeometry;
pub use ldu::LduMatrix;
pub use solve::{Controls, Performance, pbicgstab_dilu, pcg_dic, smooth_sym_gauss_seidel};
pub use vector::{VectorBc, VectorBoundary, VectorMatrix};
