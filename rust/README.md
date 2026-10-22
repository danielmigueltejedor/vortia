# Vortia — Rust port (in progress)

This directory contains an experimental Rust port of the Vortia core library.
It is **not** the primary implementation: the C++20/Qt 6 application in `src/`
remains the one used for the 2026-10 practice and production builds.

## Status

- `vortia-core`: error types, units (runtime dimensional checking), project
  manifest (serde JSON, atomic write), process runner (no shell, timeout).
- All 5 tests pass.

## Roadmap for the Rust port

1. Geometry, mesh, physics modules (mirroring `src/geometry`, `src/mesh`, `src/physics`).
2. OpenFOAM adapter (process-based, as in C++).
3. GUI: evaluate `egui`/`iced` or keep Qt via `cxx`/`slint`. Decision pending.
4. VTK interop via `vtk-rs` or FFI.

The port is incremental and does not block the C++ deliverables.