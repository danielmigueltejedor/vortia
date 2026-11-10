#!/usr/bin/env bash
# Native Poiseuille verification: write → blockMesh → steady laminar solver → sample → compare.
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

case_dir="$work/poiseuille-case"
rm -rf "$case_dir"
mkdir -p "$work"
"$write_tool" poiseuille "$case_dir" "$dist"

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

ref="$work/poiseuille_analytic.csv"
python3 - <<'PY' >"$ref"
H=0.01
L=0.1
nu=1e-3
pin=0.1
pout=0.0
dpdx=(pout-pin)/L
print("# y,u_analytic")
print("y,u")
for i in range(81):
    y=H*i/80
    u=(-dpdx)/(2*nu)*y*(H-y)
    print(f"{y:.10e},{u:.10e}")
PY

python3 "$root/benchmarks/common/compare_profiles.py" \
  --reference "$ref" \
  --candidate "$sample" \
  --format openfoam-raw \
  --component 0 \
  --rel-l2-max 0.02 \
  --rel-linf-max 0.03

# Wall force against the exact momentum balance F_x = (p_in - p_out) H W.
force_tool="$(dirname "$write_tool")/force_report"
if [[ "$dist" == "esi" && -x "$force_tool" ]]; then
  if ! "${solver[@]}" -postProcess -func forces -latestTime > log.forces 2>&1; then
    echo "forces post-processing failed:" >&2
    tail -n 60 log.forces >&2
    exit 1
  fi
  force_file="$(find "$case_dir/postProcessing/forces" -type f -name 'force*.dat' | sort | tail -n1 || true)"
  if [[ -z "$force_file" ]]; then
    echo "No forces file under postProcessing/forces" >&2
    tail -n 40 log.forces >&2
    exit 1
  fi
  "$force_tool" poiseuille "$force_file" 0.02
else
  echo "Wall-force check skipped (needs the ESI forces function object and force_report)."
fi
