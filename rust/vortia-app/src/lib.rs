//! The desktop app of Vortia.
//!
//! Everything that is not drawing lives here and is tested without a window: the
//! preferences, the recent cases, and the run of the engine on a thread. `main` and
//! [`app`] only turn that state into egui widgets in the Nodalia Design Language.

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

pub mod app;
pub mod conditions;
pub mod controldict;
pub mod convergence;
pub mod fieldview;
pub mod flowlines;
#[cfg(target_os = "linux")]
pub mod globalmenu;
pub mod numerics;
pub mod prefs;
pub mod section;
pub mod session;
pub mod summary;
pub mod surface;
pub mod viewer;
