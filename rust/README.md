# Vortia — Rust port (in progress)

This directory contains an experimental Rust port of the Vortia core library.
It is **not** the primary implementation: the C++20/Qt 6 application in `src/`
remains the one used for the 2026-10 practice and production builds.

## Status

- `vortia-core`: error type, physical units checked at compile time, vectors and
  bounding boxes, version parsing, project manifest (same schema as the C++
  application, atomic write) and process runner (no shell, timeout).
- The unit tests are the same cases as `tests/unit/test_units.cpp`,
  `test_version.cpp` and `test_project.cpp`, plus the smoke tests of the process
  runner. Not ported yet from `src/core`: host environment and logging.
- Checked on every pull request by the `Rust` workflow (format, clippy, tests).

## Roadmap for the Rust port

1. Geometry, mesh, physics modules (mirroring `src/geometry`, `src/mesh`, `src/physics`).
2. OpenFOAM adapter (process-based, as in C++).
3. GUI: evaluate `egui`/`iced` or keep Qt via `cxx`/`slint`. Decision pending.
4. VTK interop via `vtk-rs` or FFI.

The port is incremental and does not block the C++ deliverables.