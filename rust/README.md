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

Decided on 2026-11-10: the interface will be built with **egui**, the toolkit of the
other Nodalia apps based on the Craft engines, so interface components and styles can
be shared across the suite.

1. Finish `vortia-core`: host environment and logging.
2. Geometry, mesh and physics (mirroring `src/geometry`, `src/mesh`, `src/physics`).
   Physics depends on the box domain and the block-mesh parameters, so they go first.
3. Solver: OpenFOAM adapter, process-based as in C++, case writer and log parser.
4. Interop and verification (`src/interop`, `src/verification`).
5. A command-line front end over those crates, to compare results with the C++ app.
6. The egui application shell, built on the shared design language of the Nodalia apps
   ([nodalia-design](https://github.com/danielmigueltejedor/nodalia-design)) rather than
   on styles of its own.
7. The 3D viewer. VTK has no mature Rust equivalent, so this is the largest open risk:
   a wgpu renderer for meshes, contours and profiles is the planned route.

Each crate carries the same test cases as its C++ counterpart. The port is incremental
and does not block the C++ deliverables.
