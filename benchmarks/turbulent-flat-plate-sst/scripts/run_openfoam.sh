#!/usr/bin/env bash
# Turbulent flat plate, k-omega SST with wall functions: write -> blockMesh ->
# simpleFoam -> forces -> compare the friction drag with Prandtl-Schlichting.
# Exit codes: 0 pass, 1 fail/thresholds, 2 missing tools.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$root/benchmarks/common/source_openfoam.sh"

work="${1:-}"
if [[ -z "$work" ]]; then
  echo "Usage: $0 <workDir>" >&2
  exit 2
fi

write_tool="${VORTIA_BENCHMARK_WRITE_CASE:-$root/build/bin/benchmark_write_case}"
if [[ ! -x "$write_tool" ]]; then
  echo "benchmark_write_case not found. Build the tree (UI off is fine) first." >&2
  exit 2
fi
force_tool="$(dirname "$write_tool")/force_report"
if ! command -v blockMesh >/dev/null 2>&1 || ! command -v simpleFoam >/dev/null 2>&1; then
  echo "blockMesh/simpleFoam not on PATH (this benchmark needs an ESI OpenFOAM)." >&2
  exit 2
fi
[[ -x "$force_tool" ]] || { echo "force_report not found next to benchmark_write_case" >&2; exit 2; }

case_dir="$work/turbulent-plate-case"
rm -rf "$case_dir"
mkdir -p "$work"
"$write_tool" turbulent-plate "$case_dir" esi

cd "$case_dir"
if ! blockMesh > log.blockMesh 2>&1; then
  echo "blockMesh failed:" >&2; cat log.blockMesh >&2; exit 1
fi
if ! simpleFoam > log.solver 2>&1; then
  echo "simpleFoam failed:" >&2; tail -n 120 log.solver >&2; exit 1
fi
grep -E '^Time = ' log.solver | tail -n 1
grep -E 'SIMPLE solution converged' log.solver || echo "note: iteration budget reached without meeting residualControl"
grep -E 'Solving for (Ux|k|omega),' log.solver | tail -n 3

# Wall-function validity: report y+ on the plate (not a pass criterion).
simpleFoam -postProcess -func yPlus -latestTime > log.yPlus 2>&1 || true
grep -A3 'patch plate' log.yPlus | head -n 4 || true

if ! simpleFoam -postProcess -func forces -latestTime > log.forces 2>&1; then
  echo "forces post-processing failed:" >&2; tail -n 60 log.forces >&2; exit 1
fi
force_file="$(find "$case_dir/postProcessing/forces" -type f -name 'force*.dat' | sort | tail -n1 || true)"
[[ -n "$force_file" ]] || { echo "No forces file" >&2; tail -n 40 log.forces >&2; exit 1; }
"$force_tool" turbulent-plate "$force_file" 0.15
