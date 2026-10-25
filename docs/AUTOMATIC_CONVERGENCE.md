# Automatic numerical assessment

General sketch-generated compressible flow cases now assess their own numerical
state; successful process exit alone never means convergence. Nozzle assignment
results have not thereby been validated. This feature is for the existing
steady-seeking localEuler, ideal-gas, no-swirl wedge backend only.

## Interface and use

Open **Cálculo → Ajustes de cálculo → Inicialización y cálculo**. Automatic stopping is on by
default. Existing iteration/output/CPU controls are preserved; new configurations
start with a safety cap of 50,000 iterations, not a target to run unconditionally.
Geometry, mesh, material and boundaries remain general project tools. The sidebar
replaces the old configuration tabs; icons, scrollable cards, separate advanced
criteria and the Spanish catalog apply to all four stages.

The right **Evaluación del cálculo** tab opens on a general flow run. It shows
warm-up, still evolving, stabilizing, numerical convergence, invalid state, or
safety cap without convergence. Fields, the case and previous checkpoints remain
available. A cap warning offers continuation or setup revision. Model suitability
and mesh independence require separate validation; no universal physics accuracy
certificate is produced.

## Acceptance rule

Every 50 *completed, accepted* iterations, starting a new history on every run:

- Global mass error: `abs(out-in) / scale` over generic open patches.
- Local mass error: `sum(abs(div(phi))*V) / scale`; cancelling regional errors
  cannot hide behind a zero net boundary flux. MPI processor/wedge/wall/axis
  patches are excluded from the boundary throughput, not from cell continuity.
- `scale = max(in, out, 1e-9 / 1e-3)` kg/s by default. This combines an absolute
  1e-9 kg/s floor with a relative 0.1% limit and permits a quiescent equilibrium.
- Maximum per-cell relative change over the full sample interval for p, T, rho,
  vector U, and nuTilda when registered. Floors are 1 Pa, 1 K, 1e-9 kg/m³,
  1 m/s and 1e-8 m²/s respectively. Default limit: 0.1%.
- An independent accepted-step update rate for p/T/rho/U, divided by the *actual*
  local pseudo-step and multiplied by `cbrt(cellVolume)/(soundSpeed+|U|)`.
  Scalar scales are the current magnitude with the same positive floors; velocity
  uses acoustic speed plus flow speed (at least 1 m/s). Default limit: 1e-5.
  This is a normalized update rate, not an algebraic residual. It prevents tiny
  pseudo-steps alone from manufacturing a stable-looking field history.
- Positive finite p/T/rho, finite U/nuTilda in cells and boundary fields.
- At least one open boundary; all scalar reductions are collective across ranks.

Acceptance requires 500 elapsed iterations after starting/resuming and five
consecutive successful full windows (earliest stop: 700 new iterations). A failed
sample resets the count. Diagonal rho/rhoU/rhoE residuals are **not** acceptance
criteria. Criteria are editable within bounded ranges under advanced settings.
Disabling automatic stopping keeps monitoring active and runs to the safety cap.
A non-aligned final manual/cap step does not acquire an extra successful window.

`writeNow()` saves the actual final fields, even off the normal write interval.
The versioned JSON `vortia-convergence.json` and `VORTIA_CONVERGENCE` log marker
contain the final iteration and the iteration of the latest full assessment.
The GUI uses strict parsing and displays prior assessments only when the physics
fingerprint is current and the report matches the latest saved checkpoint.
Parallel assessments are published only alongside that reconstructed checkpoint.
A resumed run replaces the report and resets the history, retaining old fields.
Runtime criteria changes reuse the existing mesh; physics/sketch changes do not.

## Verification

- Policy/parser/UI/control transaction regression tests run in ordinary CTest.
- `tests/native/check_convergence.py --solver /path/to/vortiaCentralFoam --output /new/private/folder`
  is opt-in and needs a sourced OpenFOAM v2606 environment. A uniform 32-cell
  equilibrium stops at 700 in serial and four-process MPI; a pressure-driven
  startup ends at 751 as **not converged**, preserving its off-interval checkpoint.
- `FlowRunControls.NativeConvergencePipelineWhenExplicitlyRequested` exercises
  source staging, building, decomposition, progress parsing, reconstruction and
  report publication in a private case specified by `VORTIA_CONVERGENCE_PIPELINE_CASE`.

These are software and numerical-behavior checks, not validation of the nozzle
solution, turbulence choice, near-wall resolution, or mesh independence.

## Scope compared with Fluent

This release does not implement the full industrial Fluent feature set. Current
sketch flow supports ideal gas, energy, inviscid Euler with slip walls, laminar
viscosity or Spalart–Allmaras with adiabatic no-slip walls, inlet/outlet/reservoir/axis boundaries, wedge meshing,
parallel execution, checkpoint continuation, field viewing and line exports.

Still outside this general sketch workflow: the wider RANS/
transition/LES family, arbitrary 3D flow generation, transient physical time,
multiphase/cavitation, combustion/species, radiation, real gases, conjugate solid
heat transfer, rotating/sliding/overset meshes, adjoint optimization and a GPU
solver. Importing a case or visualizing 3D data is not implementing these models.
No working-looking controls are supplied for unavailable physics.

Reference for the comparison: [official Fluent capabilities](https://ansyshelp.ansys.com/public/Views/Secured/corp/v252/en/flu_ug/flu_gs_sec_gs_capabilities.html).

See [workspace and performance changes](WORKSPACE_AND_PERFORMANCE.md) for preferences, CPU sizing, deferred visualization and the native A/B evidence.
