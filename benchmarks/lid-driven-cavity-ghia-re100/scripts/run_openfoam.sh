#!/usr/bin/env bash
# Native Ghia Re=100 cavity: write → blockMesh → steady laminar solver → sample U → compare.
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

write_tool="${VORTIA_BENCHMARK_WRITE_CASE:-}"
if [[ -z "$write_tool" || ! -x "$write_tool" ]]; then
  for c in "$root/build/bin/benchmark_write_case" "$root/build-02/bin/benchmark_write_case" \
           "$root/build-quality/bin/benchmark_write_case"; do
    if [[ -x "$c" ]]; then write_tool="$c"; break; fi
  done
fi
if [[ -z "$write_tool" || ! -x "$write_tool" ]]; then
  echo "benchmark_write_case not found. Build the tree (UI off is fine) first." >&2
  exit 2
fi

if ! command -v blockMesh >/dev/null 2>&1; then
  echo "blockMesh not on PATH. Source OpenFOAM and retry." >&2
  exit 2
fi

dist=esi
solver=(simpleFoam)
if command -v simpleFoam >/dev/null 2>&1; then
  dist=esi
  solver=(simpleFoam)
elif command -v foamRun >/dev/null 2>&1; then
  dist=foundation
  solver=(foamRun -solver incompressibleFluid)
else
  echo "Neither simpleFoam nor foamRun is on PATH." >&2
  exit 2
fi

case_dir="$work/ghia-case"
rm -rf "$case_dir"
mkdir -p "$work"
"$write_tool" ghia "$case_dir" "$dist"

cd "$case_dir"
if ! blockMesh > log.blockMesh 2>&1; then
  echo "blockMesh failed:" >&2
  cat log.blockMesh >&2
  exit 1
fi
if ! "${solver[@]}" > log.solver 2>&1; then
  echo "solver (${solver[*]}) failed:" >&2
  tail -n 120 log.solver >&2
  exit 1
fi
if command -v postProcess >/dev/null 2>&1; then
  postProcess -func sampleU -latestTime > log.sampleU 2>&1 || true
fi

sample="$(find "$case_dir/postProcessing" -type f \( -name '*centreline*' -o -name '*.xy' \) 2>/dev/null | head -n1 || true)"
if [[ -z "$sample" ]]; then
  echo "No sample file under postProcessing. See logs in $case_dir" >&2
  tail -n 40 log.solver >&2 || true
  tail -n 40 log.sampleU >&2 || true
  exit 1
fi

ref="$root/benchmarks/lid-driven-cavity-ghia-re100/reference/ghia_1982_re100_u_centerline.csv"
python3 "$root/benchmarks/common/compare_profiles.py" \
  --reference "$ref" \
  --candidate "$sample" \
  --format openfoam-raw \
  --component 0 \
  --scale-abscissa 10 \
  --scale-candidate 1 \
  --rel-l2-max 0.05 \
  --rel-linf-max 0.05
