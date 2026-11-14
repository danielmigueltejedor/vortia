# Vortia as a simulation tool: fluids and heat

Status: proposed 2026-11-13, in progress. This extends
[rust-engine.md](rust-engine.md), which moves Vortia to its own engine, with
the physics Vortia is missing to be a general fluid and heat simulation tool.
[CAPABILITY_MATRIX.md](../CAPABILITY_MATRIX.md) is the honest list of what
exists today: heat transfer, LES, multiphase and conjugate heat transfer are
all "no" there, and this plan is the order in which that changes.

## How the work is ordered

1. **Usable before broad.** A solver nobody can look into is not a tool, so
   post-processing comes first and grows with every solver.
2. **Heat before exotica.** Natural convection and conjugate heat transfer are
   what people reach for next after flow. Multiphase, combustion and moving
   meshes come after them.
3. **Every step proven.** As in the engine stages: a step is done when a case
   runs on the engine and matches a reference, which is OpenFOAM on the same
   files where it has the solver, and a published benchmark otherwise. The
   case goes into `benchmarks/` and CI.
4. **Each step reaches the app.** A solver is done when its case can be set
   up, run, watched and looked at in the Rust app, not only from the command
   line. Sizes below are S (days), M (a week or two), L (several weeks).

## Roadmap

### P1. Look into the results (M, starts now)

What every later step is looked at with. In the Rust app and `vortia` CLI:

- Probes at points and monitors on patches during a run, plotted live (residuals
  are already there).
- Line and plane sampling of any field, with plots; `wallHeatFlux`,
  `wallShearStress`, `yPlus` and the force and moment coefficients on patches.
- Surface and volume integrals (mass flow, mean temperature, pressure drop).
- Slices and clipping inside the volume in the 3D viewer, streamlines, and
  vector glyphs; today it draws the boundary of the mesh coloured by a field.
- Export of any of it to CSV and VTK.

Done when the nozzle and the flat plate cases are inspected with all of the
above and the numbers equal OpenFOAM's `postProcess` on the same results.

### P2. Natural and mixed convection (L)

Buoyancy is the first piece of heat transfer, and it reuses the compressible
SIMPLE solver.

- `p_rgh` and gravity (`g`, `hRef`), Boussinesq (`rhoConst`-style fluid with a
  `beta`) and perfect gas, steady and transient. OpenFOAM's `buoyantFoam` family
  (`buoyantSimpleFoam`, `buoyantPimpleFoam`).
- Heat boundary conditions: fixed temperature, fixed flux, convection
  coefficient with ambient (`externalWallHeatFlux`), and heat sources
  (`semiImplicitSource`).
- Thermophysics library: Sutherland and polynomial transport, JANAF and
  constant-`Cp` thermodynamics, and a short list of ready fluids (air, water,
  common gases) selectable in the app.

Done when the differentially heated square cavity matches the de Vahl Davis
benchmark for Rayleigh 1e3 to 1e6 (Nusselt numbers to 1 %), and a heated
channel agrees with OpenFOAM's solver on the same files.

Progress (2026-11-14): the first slice is in the engine as
`buoyantBoussinesqSimpleFoam`: laminar, steady, Boussinesq, `p_rgh` with
`fixedFluxPressure`, `g` and `hRef`. It matches the benchmark's Nusselt number
to 0.4 % at Ra 1e3, 1.1 % at 1e4 and 1.0 % at 1e5 (2.7 % at 1e6, on a uniform
mesh that does not resolve the boundary layers); the velocity peaks agree to
within 1.5 %. The heat boundary conditions of a kinematic temperature are in too: the
`mixed` condition gives a film coefficient with an ambient temperature and, with
a reference gradient, a fixed heat flux (both checked against the analytic
conduction solution). `externalWallHeatFlux` and `semiImplicitSource` need the
conductivity and the thermophysics library, so they come with that. Left:
comparison against OpenFOAM on the same files, graded meshes for the Ra 1e6
case, the perfect-gas and transient variants, the thermophysics library, the
screens in the app.

### P3. Transient solving with an adjustable step (M)

`pimpleFoam` and `rhoPimpleFoam` equivalents with outer correctors,
`adjustTimeStep` by Courant number, second-order time schemes (backward,
Crank–Nicolson) and averaging of fields over time. Needed by P2's transient
cases, by vortex shedding and by anything unsteady.

Done when a flow past a cylinder reproduces the Strouhal number of the
reference within 2 % and the engine matches OpenFOAM's step sizes.

### P4. Conjugate heat transfer (L)

Solid and fluid regions in one case, as `chtMultiRegionFoam`:

- Region handling in the case format and the engine: `splitMeshRegions`,
  cell zones to regions, coupled interface patches.
- Solid conduction with its own material (density, `Cp`, conductivity,
  possibly anisotropic) and coupled temperature conditions across the
  interface (`turbulentTemperatureCoupledBaffleMixed`).
- In the app: regions in the mesh view, a material per region, one setup
  panel for both sides of an interface.

Done when a heated block cooled by a channel flow matches OpenFOAM's
multi-region solver on the same files, and a plate fin's temperature agrees
with the analytic fin solution.

### P5. Turbulence beyond k-ω SST (M, then L)

k-ε variants (realizable, RNG), a low-Reynolds option, more wall functions
including thermal ones (`alphatWallFunction` variants), then LES (Smagorinsky,
WALE, dynamic) and a hybrid (`kOmegaSSTDES`) with inlet turbulence generation.
Turbulent heat transfer needs the thermal wall functions of P2.

Done when the backward-facing step reattachment length and the turbulent
channel's mean profile agree with the references.

### P6. Radiation (M)

Surface-to-surface with view factors and P1 first, then the discrete
ordinates model. Needed for hot gases and furnaces; couples to P2 and P4.

Done when radiation between parallel plates and a gas-filled enclosure match
the analytic and benchmark values.

### P7. Species and passive scalars (M)

Transport of passive scalars and gas mixtures (mixing, dispersion, humid air),
without reactions. Combustion and reacting flow stay out until there is demand.

### P8. Meshing for real geometry (L)

Cut-cell hexahedral meshing from a surface (the `snappyHexMesh` approach),
prism layers on walls, and mesh quality repair. Today only structured and
imported meshes exist. Without it, none of the above reaches CAD geometry. The
pending CAD-mesh flow cases of the app are the first customer.

### P9. Performance and parallel runs (L)

Threaded assembly and solves, domain decomposition, a geometric-algebraic
multigrid solver (GAMG) for the pressure, and measured speed-ups on the
nozzle and the cavity. Parallelism is also what makes P2 to P4 practical on
real meshes.

### P10. Multiphase and moving meshes (L, last)

Volume-of-fluid (`interFoam`), then particle tracking, then dynamic meshes.
Placed last because the above covers what most heat and fluid questions need.

## Guided setup, across all of it

Each physics gets a template in the app: pick natural convection, conjugate
heat transfer or an external flow, and the app fills the fluid, the boundary
conditions, the schemes and the solver numbers, then shows what it chose. The
case editing the Rust app already has (run settings, boundary conditions,
solver numbers and schemes) is the base: a template writes those, and the
person can then change them.

## Reference cases

Each step adds the cases it is proven on to `benchmarks/`, with the reference
value it is compared to and the tolerance: the differentially heated cavity
(P2), flow past a cylinder (P3), a heated block in a channel and a plate fin
(P4), the backward-facing step (P5), parallel plates and an enclosure (P6).
The existing flat plate, cavity and nozzle cases stay.

## Interface and typography

The Rust app takes its look from `nodalia-theme`, including type. Shared
typography with Forja and Torno is decided in Nodalia Design and arrives in
Vortia through the theme, so nothing here defines a font of its own.
