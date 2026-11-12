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
  geometry, the quality report `checkMesh` gives, and structured boxes it
  can write as `polyMesh` files.
- `vortia-fields`: volume and surface fields with their boundary conditions,
  read, checked against the mesh and written back as OpenFOAM writes them.
- `vortia-fv`: the finite-volume discretisation (gradient, diffusion with
  non-orthogonal correction, upwind and linear convection, Euler time
  derivative),
  vector equations with their `A` and `H` for pressure–velocity coupling and
  under-relaxation, the explicit viscous stress, and linear solvers (PCG with
  DIC, PBiCGStab with DILU, the symmetric Gauss–Seidel smoother), with
  OpenFOAM's residual definition.
- `vortia-solvers`: solvers run on a case directory: diffusion
  (`laplacianFoam`), convection–diffusion by a given flow
  (`scalarTransportFoam`), transient incompressible flow with PISO
  (`icoFoam`), steady flow with SIMPLE (`simpleFoam`, laminar, k-ε or
  k-ω SST) and steady laminar flow of a perfect gas (`rhoSimpleFoam`).
- `vortia-cli`: the `vortia` command.

The meshing commands read and write a case's `constant/polyMesh`:
`vortia blockmesh` (the engine's `blockMesh`), `vortia gmshtofoam` (Gmsh
`.msh`, formats 1, 2 and 4), `vortia fluenttofoam` (ASCII Fluent `.msh`, 3-D and
2-D) and `vortia cgnstofoam` (CGNS, ADF format, as Fluent and ANSYS Meshing
export it).

```bash
cd rust
cargo run -p vortia-cli -- check-mesh ../examples/nozzle-practice-2026
cargo run -p vortia-cli -- fields ../examples/nozzle-practice-2026
cargo run -p vortia-cli -- solve path/to/a/case   # laplacianFoam, scalarTransportFoam, icoFoam, simpleFoam or rhoSimpleFoam
```

The interface will be built with **egui**, the toolkit other Nodalia apps use
(decided on 2026-11-10), so interface components and styles can be shared across
the suite.

## Checks

Every pull request runs `cargo fmt --check`, `cargo clippy -D warnings` and the
tests on Linux and macOS (the `Rust` workflow). The engine crates forbid
`unsafe` and deny `unwrap`, `expect`, `panic!` and unchecked indexing.
