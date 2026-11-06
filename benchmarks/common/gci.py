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


def _numerically_equal(a: float, b: float) -> bool:
    """True when a and b differ by ±0 only (treats 0.0 and -0.0 as equal)."""
    return abs(a - b) <= 0.0


def _is_numeric_zero(x: float) -> bool:
    return abs(x) <= 0.0


def _same_sign(a: float, b: float) -> bool:
    return math.copysign(1.0, a) == math.copysign(1.0, b)


def compute_gci(f1: float, f2: float, f3: float, r21: float, r32: float, fs: float = 1.25):
    if not all(math.isfinite(v) for v in (f1, f2, f3, r21, r32, fs)):
        raise ValueError("all inputs must be finite")
    if r21 <= 1.0 or r32 <= 1.0 or fs <= 0.0:
        raise ValueError("r21, r32 must be > 1 and safety factor > 0")

    eps21 = f2 - f1
    eps32 = f3 - f2

    if _numerically_equal(f1, f2) and _numerically_equal(f2, f3):
        return {
            "observed_order": 0.0,
            "extrapolated": f1 + 0.0,
            "gci12": 0.0,
            "gci23": 0.0,
            "relative_gci": True,
            "asymptotic_ratio": 1.0,
            "message": "identical solutions on all three meshes",
        }
    if _is_numeric_zero(eps21) or _is_numeric_zero(eps32):
        raise ValueError("zero consecutive difference; cannot estimate observed order")
    if not _same_sign(eps21, eps32):
        raise ValueError("oscillatory convergence; monotonic GCI does not apply")

    abs_ratio = abs(eps32 / eps21)
    equal_ratios = abs(r21 - r32) <= 1e-12
    # Equal-r studies require |ε32/ε21| > 1 (errors shrink toward the fine mesh).
    if equal_ratios and not (abs_ratio > 1.0):
        raise ValueError("equal-ratio study requires |eps32/eps21| > 1 (positive observed order)")

    # Celik et al. (2008). Outer abs on the seed only for unequal ratios.
    s = 1.0
    if equal_ratios:
        p = math.log(abs_ratio) / math.log(r21)
    else:
        p = abs(math.log(abs_ratio)) / math.log(r21)
        converged = False
        for _ in range(50):
            num = r21**p - s
            den = r32**p - s
            if not (num > 0.0 and den > 0.0 and math.isfinite(num) and math.isfinite(den)):
                raise ValueError("observed-order iteration left the valid domain")
            q = math.log(num / den)
            p_new = abs(math.log(abs_ratio) + q) / math.log(r21)
            if not math.isfinite(p_new) or p_new <= 0.0:
                raise ValueError("observed order is non-positive or non-finite")
            if abs(p_new - p) < 1e-10 * max(1.0, abs(p_new)):
                p = p_new
                converged = True
                break
            p = p_new
        if not converged:
            raise ValueError("observed-order iteration did not converge")
    if not math.isfinite(p) or p <= 0.0:
        raise ValueError("observed order is non-positive or non-finite")

    rp21 = r21**p
    rp32 = r32**p
    f_exact = (rp21 * f1 - f2) / (rp21 - 1.0)
    abs_gci12 = fs * abs(eps21) / (rp21 - 1.0)
    abs_gci23 = fs * abs(eps32) / (rp32 - 1.0)
    scale1 = abs(f1)
    scale2 = abs(f2)
    relative = scale1 > 0.0 and scale2 > 0.0
    gci12 = abs_gci12 / scale1 if relative else abs_gci12
    gci23 = abs_gci23 / scale2 if relative else abs_gci23
    if not (gci12 > 0.0):
        raise ValueError("fine-grid GCI is zero; asymptotic ratio undefined")
    asym = gci23 / (rp21 * gci12)
    return {
        "observed_order": p,
        "extrapolated": f_exact,
        "gci12": gci12,
        "gci23": gci23,
        "relative_gci": relative,
        "asymptotic_ratio": asym,
        "message": "ok" if relative else "ok (absolute GCI; QoI scale was zero)",
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
