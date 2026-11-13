//! Finite-volume discretisation and linear solvers of Vortia's CFD engine.
//!
//! - [`LduMatrix`]: sparse matrices addressed by the mesh's faces.
//! - [`solve`]: iterative solvers with OpenFOAM's residual definition.
//! - [`block`]: the Gauss–Seidel smoother cut into blocks that run on threads.
//! - [`FvGeometry`]: interpolation weights and face-gradient coefficients.
//! - [`bc`]: scalar boundary conditions, evaluated.
//! - [`ops`]: gradient, diffusion, convection and time-derivative operators.
//! - [`reconstruct`]: directed face values of limited schemes (central-upwind fluxes).
//! - [`smooth`]: `fvc::smooth`, the ratio limit on a cell field (local time stepping).
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
pub mod block;
pub mod correction;
pub mod error;
pub mod fvgeom;
pub mod ldu;
pub mod limited;
pub mod ops;
pub mod reconstruct;
pub mod scalar;
pub mod smooth;
pub mod solve;
pub mod stress;
pub mod vector;

pub use bc::{Boundary, ScalarBc};
pub use block::{smooth_sym_gauss_seidel_blocks, thread_count};
pub use error::{FvError, Result};
pub use fvgeom::FvGeometry;
pub use ldu::LduMatrix;
pub use scalar::ScalarMatrix;
pub use solve::{Controls, Performance, pbicgstab_dilu, pcg_dic, smooth_sym_gauss_seidel};
pub use vector::{VectorBc, VectorBoundary, VectorMatrix};
