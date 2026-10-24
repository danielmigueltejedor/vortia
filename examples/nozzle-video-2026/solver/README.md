# Experimental nozzle backend (OpenFOAM v2606)

This directory is an independent GPL-3.0-or-later derivative of OpenFOAM v2606 rhoCentralFoam, build _481094fdf3. Original OpenFOAM copyright/license notices and LICENSE.md are retained. Matching OpenFOAM libraries and the wmake toolchain are required. This solver is separate from Vortia's MIT core.

Local steps also account for viscous/thermal diffusion. Candidate density and absolute temperature are checked before accepting a conservative update. Rejected candidates are recalculated from their starting fields with smaller steps; no temperature clipping is applied. Sensible energy may be negative relative to its reference: its admissible bound is obtained from the thermodynamic model at a positive temperature close to zero. Only localEuler is supported. Step growth is damped after reductions.

Build with the v2606 environment loaded and VORTIA_NOZZLE_OUTPUT set to an absolute executable destination, then run wmake in this directory. Vortia supplies this destination and builds once per dedicated case. The backend is experimental until case balances and mesh dependence have been checked. Diagonal residuals of zero do not prove convergence.

Version 3 models axisymmetric flow without swirl. Internal mid-plane Uz is constrained to zero after both momentum updates; wedge face transformations remain active. Only an x-axis wedge in the xy mid-plane is supported, with noSwirl true. Restarts containing azimuthal velocity are rejected. Earlier v2 runs with unintended swirl are diagnostic, not validated results.

The paired Minmod path evaluates the shared limiting field/gradient once for
the positive and negative reconstructions, using OpenFOAM's original limiter
and interpolation arithmetic. Other reconstruction schemes use the stock
path. Set pairedReconstruction false in controlDict for a reference run, or
verifyPairedReconstruction true to compare every reconstructed face with the
stock path during a diagnostic run. The latter is intentionally slower.

Positivity acceptance and retries are collective across MPI ranks. Vortia can
run a generated CAD flow case with 1–8 CPU processes. The UI stages a bundled,
content-addressed source revision; its executable has a separate cache, so old
sources/binaries and the saved solution remain available. An old case receives
the computational update without changing its boundary dictionaries or mesh.

Parallel work is isolated under parallel-runs/run_*/. It copies the latest root
checkpoint and uses Scotch decomposition. After completion (or cancellation),
reconstructPar publishes only the latest complete checkpoint into the root
case, through an atomic directory rename. Older root checkpoints and any
pre-existing processor directories remain untouched. Intermediate partitioned
states and recovery logs stay in the run folder; include it in backups.
Reconstruction failure retains the last root checkpoint and reports failure.
MPI, decomposePar and reconstructPar from the captured environment are required.
If unavailable, select one process. More processes are not always faster.

Parallel floating-point order, linear solves and the meshWave wall-distance
approximation can cause decomposition dependence, especially in nuTilda/nut.
Compare balances, fields and wall quantities when selecting a decomposition;
software timing and reconstruction checks do not establish RANS convergence or
partition independence. No Courant limits, physical models, stopping criteria
or spatial accuracy were relaxed for these performance changes.
