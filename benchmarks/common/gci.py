#!/usr/bin/env python3
"""Roache three-grid GCI from three scalar QoI values.

Does not invent CFD results. Exits:
  0 — computed successfully
  1 — invalid inputs / non-computable order
  2 — usage error
"""
from __future__ import annotations

import argparse
import math
import sys


def compute_gci(f1: float, f2: float, f3: float, r21: float, r32: float, fs: float = 1.25):
    if not all(math.isfinite(v) for v in (f1, f2, f3, r21, r32, fs)):
        raise ValueError("all inputs must be finite")
    if r21 <= 1.0 or r32 <= 1.0 or fs <= 0.0:
        raise ValueError("r21, r32 must be > 1 and safety factor > 0")

    eps21 = f2 - f1
    eps32 = f3 - f2
    if eps21 == 0.0 and eps32 == 0.0:
        return {
            "observed_order": 0.0,
            "extrapolated": f1,
            "gci12": 0.0,
            "gci23": 0.0,
            "asymptotic_ratio": 1.0,
            "message": "identical solutions on all three meshes",
        }
    if eps21 == 0.0 or eps32 == 0.0:
        raise ValueError("zero consecutive difference; cannot estimate observed order")

    oscillatory = (eps21 * eps32) < 0.0
    ratio = abs(eps32 / eps21)
    p = math.log(ratio) / math.log(r21)
    if abs(r21 - r32) > 1e-12:
        q = math.log((r21**p - 1.0) / (r32**p - 1.0))
        p = (math.log(ratio) + q) / math.log(r21)
    if not math.isfinite(p) or p <= 0.0:
        raise ValueError("observed order is non-positive or non-finite")

    rp21 = r21**p
    rp32 = r32**p
    f_exact = (rp21 * f1 - f2) / (rp21 - 1.0)
    abs_gci12 = fs * abs(eps21) / (rp21 - 1.0)
    abs_gci23 = fs * abs(eps32) / (rp32 - 1.0)
    scale1 = abs(f1) if abs(f1) > 0.0 else 1.0
    scale2 = abs(f2) if abs(f2) > 0.0 else 1.0
    gci12 = abs_gci12 / scale1
    gci23 = abs_gci23 / scale2
    asym = abs_gci23 / (rp21 * abs_gci12) if abs_gci12 > 0.0 else 0.0
    return {
        "observed_order": p,
        "extrapolated": f_exact,
        "gci12": gci12,
        "gci23": gci23,
        "asymptotic_ratio": asym,
        "message": "oscillatory convergence" if oscillatory else "ok",
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--f1", type=float, required=True, help="QoI on finest mesh")
    ap.add_argument("--f2", type=float, required=True, help="QoI on mid mesh")
    ap.add_argument("--f3", type=float, required=True, help="QoI on coarsest mesh")
    ap.add_argument("--r21", type=float, default=2.0, help="h2/h1 (>1)")
    ap.add_argument("--r32", type=float, default=2.0, help="h3/h2 (>1)")
    ap.add_argument("--fs", type=float, default=1.25, help="safety factor (default 1.25)")
    args = ap.parse_args()
    try:
        result = compute_gci(args.f1, args.f2, args.f3, args.r21, args.r32, args.fs)
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(
        f"p={result['observed_order']:.6g} f*= {result['extrapolated']:.6g} "
        f"GCI12={result['gci12']:.6g} GCI23={result['gci23']:.6g} "
        f"asym={result['asymptotic_ratio']:.6g} ({result['message']})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
