# Vortia in Rust (in progress)

Vortia is moving to Rust, together with a CFD engine of its own built from
OpenFOAM's methods. The C++20/Qt 6 application in `src/` remains the one that
ships until the Rust side can replace it. The plan, stages and principles are
in [`docs/architecture/rust-engine.md`](../docs/architecture/rust-engine.md).

## Crates

- `vortia-core`: error type, physical units checked at compile time, vectors
  and bounding boxes, version parsing, project manifest (same schema as the C++
  application, atomic write) and process runner (no shell, timeout). Its tests
  are the cases of `tests/unit/test_{units,version,project}.cpp`.
- `vortia-foamfile`: reader for the case format: dictionaries, lists,
  dimension sets, the `polyMesh` files; ASCII, optionally gzip-compressed.
- `vortia-mesh`: the engine's polyhedral mesh: validation, face and cell
  geometry, the quality report `checkMesh` gives, and structured boxes.
- `vortia-fields`: volume and surface fields with their boundary conditions,
  read, checked against the mesh and written back as OpenFOAM writes them.
- `vortia-cli`: the `vortia` command.

```bash
cargo run -p vortia-cli -- check-mesh ../examples/nozzle-practice-2026
cargo run -p vortia-cli -- fields ../examples/nozzle-practice-2026
```

## Checks

Every pull request runs `cargo fmt --check`, `cargo clippy -D warnings` and the
tests on Linux and macOS (the `Rust` workflow). The engine crates forbid
`unsafe` and deny `unwrap`, `expect`, `panic!` and unchecked indexing.
