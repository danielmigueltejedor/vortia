#!/usr/bin/env bash
# Native Ghia Re=100 cavity: write → blockMesh → simpleFoam → sample U → compare.
# Exit codes: 0 pass, 1 fail/thresholds, 2 missing tools, 3 incomplete/aborted.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
work="${1:-}"
if [[ -z "$work" ]]; then
  echo "Usage: $0 <workDir>" >&2
  exit 2
fi

write_tool="${VORTIA_BENCHMARK_WRITE_CASE:-$root/build-02/bin/benchmark_write_case}"
if [[ ! -x "$write_tool" ]]; then
  write_tool="$root/build/bin/benchmark_write_case"
fi
if [[ ! -x "$write_tool" ]]; then
  echo "benchmark_write_case not found. Build the tree (UI off is fine) first." >&2
  exit 2
fi

if ! command -v blockMesh >/dev/null 2>&1 || ! command -v simpleFoam >/dev/null 2>&1; then
  echo "OpenFOAM (blockMesh/simpleFoam) not on PATH. Source the environment and retry." >&2
  exit 2
fi

case_dir="$work/ghia-case"
rm -rf "$case_dir"
mkdir -p "$work"
"$write_tool" ghia "$case_dir"

(
  cd "$case_dir"
  blockMesh > log.blockMesh 2>&1
  simpleFoam > log.simpleFoam 2>&1
  if command -v postProcess >/dev/null 2>&1; then
    postProcess -func sampleU -latestTime > log.sampleU 2>&1 || true
  fi
)

sample="$(find "$case_dir/postProcessing" -type f -name '*centreline*' 2>/dev/null | head -n1 || true)"
if [[ -z "$sample" ]]; then
  echo "No sample file under postProcessing. Solver may have failed; see logs in $case_dir" >&2
  exit 1
fi

ref="$root/benchmarks/lid-driven-cavity-ghia-re100/reference/ghia_1982_re100_u_centerline.csv"
# Candidate is dimensional (m, m/s); Ghia tables are normalised by H=0.1 m, U=1 m/s.
python3 "$root/benchmarks/common/compare_profiles.py" \
  --reference "$ref" \
  --candidate "$sample" \
  --format openfoam-raw \
  --component 0 \
  --scale-abscissa 10 \
  --scale-candidate 1 \
  --rel-l2-max 0.05 \
  --rel-linf-max 0.05
