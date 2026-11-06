# Lid-driven cavity — Ghia et al. Re = 100

| Field | Value |
|---|---|
| Level | Code verification (published numerical benchmark) |
| Physics | Steady 2D incompressible laminar NS |
| Geometry | Unit square scaled to \(H=0.1\,\mathrm{m}\) |
| Lid | \(U=1\,\mathrm{m}/\mathrm{s}\) on \(y=H\); other walls no-slip |
| \(\nu\) | \(0.001\,\mathrm{m}^2/\mathrm{s}\) ⇒ \(\mathrm{Re}=UH/\nu=100\) |
| Reference | Ghia, Ghia & Shin, J. Comput. Phys. **48**:387–411 (1982), Tables I–II |
| Vortia API | `vortia::verification::makeGhiaCavityCase`, `ghiaRe100UCenterline`, `ghiaRe100VCenterline` |

## Hypotheses

- Incompressible Newtonian fluid; laminar; no turbulence model.
- 2D via `empty` patches in \(z\).
- Steady state (SIMPLE / `simpleFoam` path).
- Comparison uses **normalised** centreline profiles \((u/U,\,v/U)\) vs Ghia tables.

## Acceptance (set a priori)

Pending first mesh-independence study. Provisional targets for a \(\ge 64^2\) cell mesh after algebraic convergence (to be confirmed or tightened in Milestone B once native results exist):

| Quantity | Metric | Provisional threshold |
|---|---|---|
| \(u(x=H/2)\) | \(\mathrm{rel}\,L_\infty\) vs Ghia Table I | \(< 5\%\) of \(U\) |
| \(v(y=H/2)\) | \(\mathrm{rel}\,L_\infty\) vs Ghia Table II | \(< 5\%\) of \(U\) |

**No claim of compliance is made until native compare + GCI are recorded.** Use `benchmarks/common/gci.py` on a centreline metric (e.g. mid-cavity \(u\)) from three meshes (\(N\), \(2N\), \(4N\)).

## Software checks (always)

`GhiaCavity.*` tests lock the published mid-cavity sample \(u(0.5)=-0.20581\) and \(\mathrm{Re}=100\) for the default case.

## Citation

See `reference/SOURCE.md`.

## Harness

Use `benchmark_write_case` and `scripts/run_openfoam.sh` (requires OpenFOAM on PATH). CTest skips with code 2 when solvers are absent.
