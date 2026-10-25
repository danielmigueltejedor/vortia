# Workflow, preferences and conservative-update efficiency

## Workflow

The ribbon follows five stages: Home, Geometry, Physics, Calculation, Results.
Geometry contains one sketch editor, a sketch-file/standalone-mesh menu, mesh
controls and CFD mesh generation. Physics contains the material/model and
boundary editors. Calculation contains run controls, run and cancel. Results
contains one field selector, line sampling, cell edges, latest-result conversion,
file opening and a camera menu. Repeated geometry, setup, theme and camera
buttons have been removed from the ribbon; the native menus retain their actions.
The project tree selects the matching stage. The unrelated right properties dock is hidden while editing the sketch; calculation assessment opens for runs/results according to the preference. `Ctrl+K` (Command+K on macOS) searches
published, enabled tools. Each ribbon action appears only once.

Embedded sketch file controls and standalone block-mesh exporters live in the
Sketch files menu. This preserves those tools while keeping project save separate
from sketch export. Native buttons expose the setup/preferences categories to
accessibility APIs and keyboard focus; Alt+1 through Alt+4 select categories.

General sketch-flow cases show numerical assessment in the right dock instead of
the unrelated cavity properties editor. The shorter experimental-CFD notice still
distinguishes numerical acceptance from physical/mesh validation.

## Preferences

Home → Preferences, or the platform Preferences shortcut, opens four categories:

- Appearance: language, dark/light theme, comfortable/compact tools, interface
  size, and remembering the window/panel layout.
- Sketch: visible grid, dimension annotations, snapping, snap spacing and zoom
  sensitivity. These change interaction/display and preserve existing geometry.
- Calculation: default CPU allocation, safety cap, checkpoint interval, automatic
  visualization, showing assessment/console on run, and console history size.
- Engine/storage: optional OpenFOAM root, installed-engine discovery and storage
  for new calculations. Engine/storage changes are disabled during a run.

CPU/cap/checkpoint defaults apply only when a flow configuration is new; saved
project values take priority. Automatic CPU selection (`processes: 0`) avoids MPI
for meshes below 6,000 cells, conservatively budgets around 3,000 cells per rank,
and uses at most eight hardware threads. It selects four on the 12,580-cell test
mesh, where the measured desktop overhead was lower than eight. Explicit process
counts are preserved. Missing mesh metadata falls back to serial.

Storage is an absolute path without spaces, a limitation of the current OpenFOAM
build workflow. Existing cases are never moved by a preference change. Prepared
projects record their engine identity, separate from the physics fingerprint.
Selecting a different engine produces a fresh revision instead of reusing binaries
and dynamic boundary libraries from the previous runtime. Old source projects
migrate their recorded backend identity; imported older cases with an explicitly
selected custom engine require a fresh revision from their editable project.

Automatic visualization can be disabled to skip foamToVTK after calculation.
Native fields are still saved, including the actual final checkpoint. View latest
result exports them on demand and reuses an up-to-date export. A cached VTK export
whose series time differs from the latest native checkpoint is not automatically
loaded as the current result. Older exports remain available explicitly.

## Native efficiency

The density, conservative momentum and conservative energy equations use a
localEuler diagonal time matrix plus explicit divergence. The direct path solves
that same diagonal equation per cell, retaining OpenFOAM's Vsc scaling, arithmetic
order, old-time fields and boundary correction. It avoids three fvMatrix/solver
allocations and dispatches. The implicit velocity/internal-energy viscous solves,
flux scheme, limiter, positive-candidate rejection, timestep limits, no-swirl
constraint and numerical convergence criteria are retained. Moving meshes use the
reference fvMatrix path; the supported generated domain is static.

`directConservativeUpdates false` selects the reference for regression tests.
The default diagnostic interval is ten iterations. Courant/time-scale reductions
and timing output are diagnostic only; skipping them between reports does not
change the timestep restriction. `vortiaLogInterval 1` enables detailed diagnostics.
The actual final iteration is always logged. Invalid-state messages and candidate
rejections remain visible. Newly generated dictionaries suppress algebraic-solver
chatter with `log 0`; imported fvSolution files are preserved. A stable
`VORTIA_TIMING` JSON marker separates compute/checkpoint time from MPI startup and
shutdown in benchmarks. Diagnostic controls are excluded from mesh fingerprints.

Reusable step snapshots were investigated but did not consistently reduce wall
time on this desktop, so `reuseStepSnapshots` remains **false** by default. This
experimental native switch is not exposed as an application option.

## Shared compiled-engine cache

The app keys compiled engines by bundled source revision plus an OpenFOAM runtime
hash (installation, compiler options and core library metadata). New case revisions
reuse a complete executable from the application cache rather than compiling the
same engine repeatedly. Each case retains its own executable copy. Publication is
atomic; cache storage failure falls back to normal case-local compilation. A runtime
change selects a separate local binary/cache key, preserving older binaries.
Dynamic boundary conditions still follow OpenFOAM's own per-case compilation cache;
this feature does not promise to remove every first-run compilation step.

Fake-backend regressions verify cross-case reuse and runtime separation without
running CFD. The GUI/native smoke run checks the actual bundled-source path.

## Evidence and limits

`tests/native/compare_conservative_updates.py` runs a private, frozen-checkpoint
ABBA paired comparison (reference, fast, fast, reference). MPI runs reuse
exactly the same Scotch cell addressing and initial partitioned fields. Each run
keeps its files and logs. The script stops its own process group if another native
Vortia calculation starts. The benchmark runs on a shared desktop and should not
be interpreted as universal throughput or a converged nozzle solution.

On a 12,580-cell nozzle restart, 500 further steps, identical save frequency and
four MPI ranks, mean end-to-end time was 9.028 s (reference) versus 7.745 s (fast):
about **16.6% faster throughput**, with **21.6% higher compute/checkpoint throughput**.
Saved p, T, rho, U and nuTilda values matched exactly at 12-digit output precision.
At eight ranks the compute gain was about 9.3%, while end-to-end timing was about
3% worse in that sample because launcher/desktop overhead matters. A 500-step
serial comparison was effectively flat (about 1.7% end-to-end difference). Short
200-step samples were noisier. These mixed results motivate automatic rank sizing
and measured claims, rather than promising the same gain on every mesh.

## Inviscid model

The general model selector now includes Inviscid (Euler · slip walls). It writes
zero dynamic viscosity, a finite transport Pr placeholder with zero conductivity,
laminar/no-turbulence transport and slip wall velocity. Viscous momentum/energy
diffusion is consequently skipped. Molecular weight, Cp and the energy equation
remain active. Stored nonzero transport inputs are disabled in this mode and
retained for switching back; they are not used by the inviscid calculation.
Laminar retains viscosity and no-slip walls and is not an inviscid approximation.

Preparer regressions verify transport and wall types for all three modes. Native
OpenFOAM v2606 tests verify a uniform 100 m/s Euler flow retains tangential wall
velocity and accepts numerical convergence at 700, alongside the earlier uniform
rest, MPI and unsteady pressure-startup rejection cases. These tests establish
implementation behavior, not validation of a supersonic nozzle or the assignment.
Full Fluent parity, additional turbulence/multiphase/combustion/radiation models,
mesh independence and a validated assignment solution remain outside this change.
