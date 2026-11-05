#!/usr/bin/env bash
# Optional native run for Ghia cavity verification.
# Does NOT fabricate results. Exits 2 if OpenFOAM is unavailable.
set -euo pipefail

echo "Ghia Re=100 cavity OpenFOAM harness (Milestone B native path)."
echo "Reference tables: benchmarks/lid-driven-cavity-ghia-re100/reference/"
echo "Case factory: vortia::verification::makeGhiaCavityCase"
echo

if ! command -v simpleFoam >/dev/null 2>&1 && ! command -v icoFoam >/dev/null 2>&1; then
  echo "OpenFOAM solvers not on PATH. Source the OpenFOAM environment and retry." >&2
  exit 2
fi

echo "OpenFOAM appears available, but automated write→run→centreline sample→Ghia"
echo "compare is not yet complete in this script. Do not treat this as a passed benchmark."
exit 3
