# Experimental nozzle backend (OpenFOAM v2606)

This directory is an independent GPL-3.0-or-later derivative of OpenFOAM v2606 rhoCentralFoam, build _481094fdf3. Original OpenFOAM copyright/license notices and LICENSE.md are retained. Matching OpenFOAM libraries and the wmake toolchain are required. This solver is separate from Vortia's MIT core.

Local steps also account for viscous/thermal diffusion. Candidate density and absolute temperature are checked before accepting a conservative update. Rejected candidates are recalculated from their starting fields with smaller steps; no temperature clipping is applied. Sensible energy may be negative relative to its reference: its admissible bound is obtained from the thermodynamic model at a positive temperature close to zero. Only localEuler is supported. Step growth is damped after reductions.

Build with the v2606 environment loaded and VORTIA_NOZZLE_OUTPUT set to an absolute executable destination, then run wmake in this directory. Vortia supplies this destination and builds once per dedicated case. The backend is experimental until case balances and mesh dependence have been checked. Diagonal residuals of zero do not prove convergence.

Version 3 models axisymmetric flow without swirl. Internal mid-plane Uz is constrained to zero after both momentum updates; wedge face transformations remain active. Only an x-axis wedge in the xy mid-plane is supported, with noSwirl true. Restarts containing azimuthal velocity are rejected. Earlier v2 runs with unintended swirl are diagnostic, not validated results.
