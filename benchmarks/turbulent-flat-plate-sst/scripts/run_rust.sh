#!/usr/bin/env bash
# Turbulent flat plate, k-omega SST with wall functions, on Vortia's own Rust
# engine: write -> vortia blockmesh -> vortia solve (simpleFoam) -> forces ->
# compare the friction drag with Prandtl-Schlichting.
# The forces are integrated by OpenFOAM's post-processing (the engine has no
# forces function yet), so an ESI OpenFOAM is still needed for that step.
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
[[ -x "$force_tool" ]] || { echo "force_report not found next to benchmark_write_case" >&2; exit 2; }
vortia="${VORTIA_RUST_CLI:-$root/rust/target/release/vortia}"
if [[ ! -x "$vortia" ]]; then
  echo "vortia (Rust) not found: cargo build --release -p vortia-cli in rust/" >&2
  exit 2
fi
if ! command -v simpleFoam >/dev/null 2>&1; then
  echo "simpleFoam not on PATH (needed for the forces post-processing)." >&2
  exit 2
fi

case_dir="$work/turbulent-plate-rust"
mkdir -p "$work"
if [[ -e "$case_dir" ]]; then
  echo "$case_dir already exists; give an empty work directory." >&2
  exit 2
fi
"$write_tool" turbulent-plate "$case_dir" esi

cd "$case_dir"
if ! "$vortia" blockmesh . > log.blockMesh 2>&1; then
  echo "vortia blockmesh failed:" >&2; cat log.blockMesh >&2; exit 1
fi
if ! "$vortia" solve . > log.solver 2>&1; then
  echo "vortia solve failed:" >&2; tail -n 60 log.solver >&2; exit 1
fi
grep -E '^Time = ' log.solver | tail -n 1
grep -E 'SIMPLE solution converged' log.solver || echo "note: iteration budget reached without meeting residualControl"

simpleFoam -postProcess -func yPlus -latestTime > log.yPlus 2>&1 || true
grep -A3 'patch plate' log.yPlus | head -n 4 || true

if ! simpleFoam -postProcess -func forces -latestTime > log.forces 2>&1; then
  echo "forces post-processing failed:" >&2; tail -n 60 log.forces >&2; exit 1
fi
force_file="$(find "$case_dir/postProcessing/forces" -type f -name 'force*.dat' | sort | tail -n1 || true)"
[[ -n "$force_file" ]] || { echo "No forces file" >&2; tail -n 40 log.forces >&2; exit 1; }
"$force_tool" turbulent-plate "$force_file" 0.15
