#!/usr/bin/env python3
"""Compare a sampled 1-D profile to a reference CSV.

Does not invent data. Exits:
  0 — within thresholds
  1 — comparison failed thresholds or I/O/parse error
  2 — usage error
"""
from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path


def load_xy(path: Path) -> list[tuple[float, float]]:
    rows: list[tuple[float, float]] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        line = line.replace(",", " ")
        parts = line.split()
        if len(parts) < 2:
            continue
        try:
            x = float(parts[0])
            y = float(parts[1])
        except ValueError:
            continue  # header
        rows.append((x, y))
    if len(rows) < 2:
        raise ValueError(f"need ≥2 numeric rows in {path}")
    rows.sort(key=lambda p: p[0])
    for i in range(1, len(rows)):
        if not rows[i][0] > rows[i - 1][0]:
            raise ValueError(f"non-increasing abscissae in {path}")
    return rows


def interpolate(sorted_xy: list[tuple[float, float]], queries: list[float]) -> list[float]:
    out: list[float] = []
    xs = [p[0] for p in sorted_xy]
    ys = [p[1] for p in sorted_xy]
    for q in queries:
        if q <= xs[0]:
            out.append(ys[0])
            continue
        if q >= xs[-1]:
            out.append(ys[-1])
            continue
        # binary search upper bound
        lo, hi = 0, len(xs) - 1
        while lo + 1 < hi:
            mid = (lo + hi) // 2
            if xs[mid] <= q:
                lo = mid
            else:
                hi = mid
        t = (q - xs[lo]) / (xs[hi] - xs[lo])
        out.append(ys[lo] + t * (ys[hi] - ys[lo]))
    return out


def metrics(ref: list[float], cand: list[float]) -> tuple[float, float, float, float]:
    n = len(ref)
    sum_sq = 0.0
    sum_sq_ref = 0.0
    linf = 0.0
    max_abs_ref = 0.0
    for a, b in zip(ref, cand):
        e = b - a
        sum_sq += e * e
        sum_sq_ref += a * a
        linf = max(linf, abs(e))
        max_abs_ref = max(max_abs_ref, abs(a))
    l2 = math.sqrt(sum_sq / n)
    ref_norm = math.sqrt(sum_sq_ref / n)
    rel_l2 = (l2 / ref_norm) if ref_norm > 0 else 0.0
    rel_linf = (linf / max_abs_ref) if max_abs_ref > 0 else 0.0
    return l2, linf, rel_l2, rel_linf


def parse_openfoam_raw_u(path: Path, component: int) -> list[tuple[float, float]]:
    """Parse foam sample raw file: x y z Ux Uy Uz (or similar)."""
    rows: list[tuple[float, float]] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.replace(",", " ").split()
        if len(parts) < 4 + component:
            continue
        try:
            # Prefer first column as abscissa when axis=y or x was requested;
            # OpenFOAM raw with axis y writes: y Ux Uy Uz  OR  x y z Ux Uy Uz
            floats = [float(p) for p in parts]
        except ValueError:
            continue
        if len(floats) >= 6:
            # x y z Ux Uy Uz
            abscissa = floats[1] if component == 0 else floats[0]
            value = floats[3 + component]
        elif len(floats) >= 4:
            abscissa = floats[0]
            value = floats[1 + component]
        else:
            continue
        rows.append((abscissa, value))
    if len(rows) < 2:
        raise ValueError(f"could not parse OpenFOAM sample file {path}")
    rows.sort(key=lambda p: p[0])
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--reference", type=Path, required=True)
    ap.add_argument("--candidate", type=Path, required=True)
    ap.add_argument("--component", type=int, default=0, help="U component for OpenFOAM raw (0=x,1=y)")
    ap.add_argument("--format", choices=("csv", "openfoam-raw"), default="csv")
    ap.add_argument("--rel-l2-max", type=float, required=True)
    ap.add_argument("--rel-linf-max", type=float, required=True)
    ap.add_argument("--scale-candidate", type=float, default=1.0,
                    help="Multiply candidate values (e.g. 1/U_lid for Ghia normalisation)")
    ap.add_argument("--scale-abscissa", type=float, default=1.0,
                    help="Multiply candidate abscissa (e.g. 1/H)")
    args = ap.parse_args()

    try:
        ref = load_xy(args.reference)
        if args.format == "csv":
            cand = load_xy(args.candidate)
        else:
            cand = parse_openfoam_raw_u(args.candidate, args.component)
        cand = [(x * args.scale_abscissa, y * args.scale_candidate) for x, y in cand]
        queries = [x for x, _ in ref]
        cand_at_ref = interpolate(cand, queries)
        ref_vals = [y for _, y in ref]
        l2, linf, rel_l2, rel_linf = metrics(ref_vals, cand_at_ref)
    except Exception as exc:  # noqa: BLE001 — surface any parse/IO issue to CI
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    print(f"n={len(ref_vals)} L2={l2:.6e} Linf={linf:.6e} relL2={rel_l2:.6e} relLinf={rel_linf:.6e}")
    print(f"thresholds: relL2<={args.rel_l2_max} relLinf<={args.rel_linf_max}")
    if rel_l2 > args.rel_l2_max or rel_linf > args.rel_linf_max:
        print("FAIL: errors exceed a-priori thresholds", file=sys.stderr)
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
