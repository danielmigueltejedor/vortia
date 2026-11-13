//! Vortia, the desktop app of the Rust engine.

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

use vortia_app::app::{self, VortiaApp};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: app::viewport(),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Vortia",
        options,
        Box::new(|cc| {
            // `vortia-app CASE [--run]`: open a case, and start it at once.
            let mut args = std::env::args_os().skip(1);
            let case = args.next();
            let run = args.next().is_some_and(|a| a == "--run");
            Ok(Box::new(VortiaApp::new(&cc.egui_ctx, case, run)))
        }),
    )
}
