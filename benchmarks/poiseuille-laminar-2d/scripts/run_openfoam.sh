#!/usr/bin/env bash
# Optional native run for Poiseuille verification.
# Does NOT fabricate results. Exits 2 if OpenFOAM / vortia tooling is unavailable.
set -euo pipefail

echo "Poiseuille OpenFOAM harness (Milestone B native path)."
echo "Status: case generation is available via vortia::verification::makePoiseuilleCase"
echo "        and CaseWriter; automated line sampling + profile compare is not yet"
echo "        wired in this script."
echo

if ! command -v simpleFoam >/dev/null 2>&1 && ! command -v icoFoam >/dev/null 2>&1; then
  echo "OpenFOAM solvers not on PATH. Source the OpenFOAM environment and retry." >&2
  exit 2
fi

echo "OpenFOAM appears available, but the automated write→run→sample→compare"
echo "pipeline is intentionally incomplete until a reproducible CI host is designated."
echo "Do not treat this message as a passed benchmark."
exit 3
