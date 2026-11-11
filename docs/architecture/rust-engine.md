# Vortia in Rust, with its own CFD engine

Status: in progress, started 2026-11-11. This is the plan of record for moving
all of Vortia to Rust and replacing the bundled OpenFOAM with an engine of
Vortia's own, built from OpenFOAM's methods.

## Why

Vortia today drives OpenFOAM as a separate program. That works, and it has
three costs that do not go away: installers carry a full OpenFOAM, MPI and a
mesher; every solver change means a C++ build against OpenFOAM's tree; and the
app only knows what a run did by parsing its log. An engine in the same
language as the app removes all three, and lets the interface ask the solver
questions directly instead of reading text.

## Principles

- **Same case format.** The engine reads and writes `0/`, `constant/` and
  `system/` exactly as OpenFOAM does. A case can be run by either, and results
  compared file by file. This is what makes each step verifiable.
- **Same methods, re-expressed.** Discretisation, mesh geometry and the
  pressure-velocity algorithms follow OpenFOAM's, so results agree to rounding
  or to solver tolerance, not merely "look similar". Each module that derives
  from OpenFOAM says which part of OpenFOAM in its header.
- **Licence.** OpenFOAM is GPL-3.0-or-later and so is Vortia, so deriving from
  it is allowed. The derived modules keep OpenFOAM's copyright notice next to
  ours. Decision D2 in [research.md](research.md) (OpenFOAM only as a separate
  process, to keep the licences apart) is superseded: Vortia was already GPL.
- **Never crash.** Every crate forbids `unsafe` and denies `unwrap`, `expect`,
  `panic!` and unchecked indexing outside tests. Case files are hostile input:
  nesting is bounded and a declared list length never decides an allocation.
- **One stage at a time, each proven.** A stage is done when a case runs on
  the engine and matches OpenFOAM on the same case within a stated tolerance.
  OpenFOAM stays bundled as the reference until every solver Vortia offers has
  passed; then it leaves the installers.

## Crates

All under [`rust/`](../../rust), one workspace:

| Crate | Role | State |
|---|---|---|
| `vortia-core` | Units, vectors, project manifest, processes | Ported from `src/core` |
| `vortia-foamfile` | Case format: dictionaries, lists, `polyMesh`; ASCII and `.gz` | Done (binary: next) |
| `vortia-mesh` | Polyhedral mesh, geometry, quality report, structured boxes | Done |
| `vortia-fields` | Volume and surface fields, boundary conditions, time directories | Next |
| `vortia-fv` | Finite-volume operators (`fvc`, `fvm`), LDU matrices, linear solvers | Planned |
| `vortia-solvers` | Laplacian, PISO/SIMPLE incompressible, k-ω SST, compressible | Planned |
| `vortia-cli` | `vortia` command: one subcommand per stage | `check-mesh` |
| app | Interface over the crates | Planned (toolkit below) |

## Stages

| # | Stage | Done when |
|---|---|---|
| E1 | Case format reader, mesh, geometry, `checkMesh` | The nozzle wedge reads with OpenFOAM's counts and its analytic volume to 1e-9 ✅ |
| E2 | Fields and boundary conditions; writing time directories | A field written by the engine is read back by OpenFOAM and ParaView |
| E3 | Operators: gradient, divergence, Laplacian with non-orthogonal correction, interpolation schemes | Each operator matches its analytic result on boxes and skewed meshes |
| E4 | LDU matrix and linear solvers: PCG/DIC, PBiCGStab/DILU, Gauss–Seidel, then GAMG | Same iterations and residuals as OpenFOAM on the same system |
| E5 | First solver: steady diffusion (`laplacianFoam`) | Matches OpenFOAM and the analytic profile |
| E6 | Incompressible: PISO transient, SIMPLE steady | Ghia cavity and Poiseuille, the existing native cases |
| E7 | Turbulence: k-ω SST with wall functions | Flat plate against the correlation already used |
| E8 | Compressible nozzle (the current in-house central scheme) | The nozzle cases in `examples/` |
| E9 | Meshing: multi-block `blockMesh` with grading and wedges, Gmsh and Fluent import | Same meshes as today's pipeline |
| E10 | Parallel assembly and solves (threads first, decomposition later) | Speed-up measured on the nozzle |

The interface port runs alongside, over the same crates: a command-line front
end first (to compare with the C++ app), then the application window, then the
3D viewer (meshes, contours, profiles), which is the largest open risk because
VTK has no Rust equivalent.

## Interface toolkit

To be confirmed. Both follow Nodalia Design: egui with `nodalia-theme`, or
GTK 4 with `nodalia-gtk` as Vinilo does. On Linux the window must export its
menus over D-Bus to the shell's global menu, which GTK does natively.
