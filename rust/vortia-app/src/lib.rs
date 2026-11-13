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
pub mod convergence;
pub mod prefs;
pub mod session;
