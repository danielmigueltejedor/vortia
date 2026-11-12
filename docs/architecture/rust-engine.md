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
| `vortia-fields` | Volume and surface fields, boundary conditions, time directories | Read and write done; evaluating conditions comes with E3 |
| `vortia-fv` | Finite-volume operators, LDU matrices, linear solvers | Diffusion, Euler time derivative, Gauss gradient, PCG/DIC |
| `vortia-solvers` | Laplacian, PISO/SIMPLE incompressible, k-ω SST, compressible | Diffusion (`laplacianFoam`), transport (`scalarTransportFoam`), PISO (`icoFoam`), SIMPLE with k-ε and k-ω SST (`simpleFoam`), laminar compressible SIMPLE/SIMPLEC (`rhoSimpleFoam`) |
| `vortia-cli` | `vortia` command: one subcommand per stage | `check-mesh`, `fields`, `solve` |
| app | Interface over the crates | Planned (toolkit below) |

## Stages

| # | Stage | Done when |
|---|---|---|
| E1 | Case format reader, mesh, geometry, `checkMesh` | The nozzle wedge reads with OpenFOAM's counts, its analytic volume to 1e-9, and `checkMesh`'s volumes, areas, non-orthogonality and skewness to the digits it prints ✅ |
| E2 | Fields and boundary conditions; writing time directories | Every nozzle field round-trips unchanged, and OpenFOAM (`foamToVTK`) reads what the engine wrote. Read/write ✅; evaluating conditions moves to E3 |
| E3 | Operators: gradient, divergence, Laplacian with non-orthogonal correction, interpolation schemes | Each operator matches OpenFOAM on the same mesh. Gradient, corrected Laplacian, Euler, and convection with `Gauss upwind` and `Gauss linear` ✅; bounded higher-order schemes (`linearUpwind`, limited) next |
| E4 | LDU matrix and linear solvers: PCG/DIC, PBiCGStab/DILU, Gauss–Seidel, then GAMG | Same iterations and residuals as OpenFOAM on the same system. PCG/DIC, PBiCGStab/DILU and `smoothSolver` with `symGaussSeidel` ✅ |
| E5 | First solver: diffusion (`laplacianFoam`), steady and transient | On the nozzle wedge with non-orthogonal correction, every solve takes OpenFOAM v1912's iteration count and the temperatures agree to 1e-13 ✅. Convection–diffusion (`scalarTransportFoam`) in the nozzle's own flow, with its `phi` or one built from `U`: same iteration counts, agreement to 1e-14 ✅ |
| E6 | Incompressible: PISO transient, SIMPLE steady | Ghia cavity and Poiseuille, the existing native cases. PISO (`icoFoam`) on the lid-driven cavity, meshed by the engine and run by OpenFOAM v1912 on the same files: all 400 solves of the tutorial's 100 steps take OpenFOAM's iteration count, and `U`, `p` and `phi` agree to 1e-14 ✅. SIMPLE (`simpleFoam`, laminar) on the same cavity run to steady state: it stops on the residual control after the same 114 iterations as OpenFOAM, all 456 solves take its iteration count, and the fields agree to 1e-14 ✅ |
| E7 | Turbulence: k-ω SST with wall functions | Flat plate against the correlation already used |
| E8 | Compressible nozzle (`rhoSimpleFoam`, as the cases in `examples/` run) | The nozzle cases in `examples/`. First step: laminar `rhoSimpleFoam` with a perfect gas (`hePsiThermo`, `hConst`, `const`), internal energy or enthalpy, SIMPLE or SIMPLEC, on a heated channel meshed by the engine and run by OpenFOAM v1912 on the same files: with internal energy and SIMPLEC it converges after the same 166 iterations as OpenFOAM, with enthalpy and plain SIMPLE the 40 iterations run match, every solve before the pressure reaches its rounding floor takes OpenFOAM's iteration count, and `U`, `T`, `p` and `ρ` agree to 1e-9 or better ✅. Second step: the conditions that make the nozzle's flow by pressure, `totalPressure` and `totalTemperature` at the inlet with `pressureInletVelocity`, and `pressureInletOutletVelocity` with `inletOutlet` at the outlet, updated at the points where OpenFOAM updates them; the same channel driven this way agrees with OpenFOAM in every solve's iteration count over 100 iterations and in the fields to 1e-10 ✅. Third step: the nozzle's own mesh, an axisymmetric wedge, laminar with the pressure-driven conditions: the wedge's `transform` condition for vectors (its diagonal and explicit parts in the matrices, `HbyA` turned onto the side, the gradient tensor turned for the viscous stress), where 60 iterations take OpenFOAM's iteration count in every solve of `Ux`, `Uy`, `e` and `p` and the fields agree to 1e-9 or better ✅ (`Uz`, some 1e-12 m/s of noise where the mesh is not exactly symmetric, differs by one iteration in a few solves). Fourth step: the schemes the nozzle's own `fvSchemes` asks for, `linearUpwind` over a `cellLimited Gauss linear 1` gradient (implicit upwind, the gradient correction explicit, for the velocity, the energy and the kinetic energy; the limited gradient also feeds the viscous stress), and `$name` macros in `fvSchemes`; the wedge nozzle's 60 iterations match OpenFOAM in every solve of `Ux`, `Uy`, `e` and `p`, and in the fields to 1e-9 or better ✅. Fifth step: the non-orthogonal correction, `Gauss linear corrected` for the Laplacians of the velocity (over the limited `grad(U)`), the energy and the pressure, and `corrected` for `snGrad(p)`, on the nozzle's mesh, whose faces are up to 7° off orthogonal; the pressure matrix's explicit correction goes into its face flux too, and with `nNonOrthogonalCorrectors 2` the 60 iterations still take OpenFOAM's counts, temperature agrees to 1e-14, pressure to 1e-13 and density to 6e-14 ✅. (Laminar `rhoSimpleFoam` asks for `laplacian(alphahe,e)`, not `alphaEff`, and its energy gradient reads the patch values the last solve left, as OpenFOAM's does.) Sixth step: compressible k-ω SST with the nozzle's wall functions (`kqRWallFunction`, `omegaWallFunction`, `nutkWallFunction`, `compressible::alphatWallFunction`) and the mixed inlets (`turbulentIntensityKineticEnergyInlet`, `turbulentMixingLengthFrequencyInlet`): the equations are weighted by ρ, `divU` comes from `phi/ρ_f`, `ν = μ/ρ`, `alphat = ρ ν_t/Prt`, and the turbulence is corrected after the energy and pressure equations. The wedge faces of `ν_t` and the other turbulence fields follow the cells as `correctBoundaryConditions` does, and the velocity gradient at the first validation takes the outlet's file value with a normal gradient of zero, as OpenFOAM's `pressureInletOutletVelocity` does before its first update. The example nozzle (real case, its own schemes, 7500 cells) takes OpenFOAM's iteration count in every solve of the first 30 iterations (210 of 210) and in 696 of 700 over 100, and the fields agree to 1e-10 after 3 iterations and to 1e-5 or better after 100; the small drift comes from the cell-limited gradients, in whose state OpenFOAM keeps something the files it writes do not hold (a restart from them agrees with this engine, not with the uninterrupted run) ✅. Next: meshing (E9) |
| E9 | Meshing: multi-block `blockMesh` with grading and wedges, Gmsh and Fluent import | Same meshes as today's pipeline |
| E10 | Parallel assembly and solves (threads first, decomposition later) | Speed-up measured on the nozzle |

The interface port runs alongside, over the same crates: a command-line front
end first (to compare with the C++ app), then the application window, then the
3D viewer (meshes, contours, profiles), which is the largest open risk because
VTK has no Rust equivalent.

## Interface toolkit

egui, styled with `nodalia-theme` from Nodalia Design (decided 2026-11-11).
The 3D viewer renders with wgpu inside the egui window. On Linux the menus
must also be exported over D-Bus to the shell's global menu; egui does not do
that by itself, so the app carries a small exporter of its own.
