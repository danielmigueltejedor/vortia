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
            // `vortia-app CASE [--run] [--mesh] [--field NAME]`: open a case, start it at once, show its
            // mesh, or show it coloured with a field.
            let mut args = std::env::args_os().skip(1);
            let case = args.next();
            let flags: Vec<_> = args.collect();
            let has = |flag: &str| flags.iter().any(|a| a == flag);
            let mut app = VortiaApp::new(&cc.egui_ctx, case, has("--run"));
            if let Some(at) = flags.iter().position(|a| a == "--field")
                && let Some(field) = flags.get(at + 1)
            {
                app.show_mesh_field(&field.to_string_lossy());
            } else if has("--mesh") {
                app.show_mesh();
            }
            Ok(Box::new(app))
        }),
    )
}
