# Poiseuille laminar channel (2D)

| Field | Value |
|---|---|
| Level | Code verification (analytic) |
| Physics | Steady laminar incompressible NS, Newtonian fluid |
| Geometry | Channel \(0 \le x \le L\), \(0 \le y \le H\), thin \(z\) with `empty` |
| Reference | Exact developed profile \(u(y)=\frac{-dp/dx}{2\nu}\,y(H-y)\) |
| Vortia API | `vortia::verification::makePoiseuilleCase` |
| Default params | \(L=0.1\,\mathrm{m}\), \(H=0.01\,\mathrm{m}\), \(\nu=10^{-3}\,\mathrm{m}^2/\mathrm{s}\), \(p_\mathrm{in}=0.1\), \(p_\mathrm{out}=0\) (kinematic) |

## Hypotheses

- Fully developed, steady, laminar, parallel flow (\(v=w=0\), \(\partial_x u = 0\)).
- Constant \(\nu\); no body force other than the imposed pressure gradient.
- 2D approximation via `empty` front/back patches.

## Acceptance (set a priori)

| Metric | Threshold | Rationale |
|---|---|---|
| Wall \(u\) | \(\|u\| < 10^{-8}\,U_\mathrm{max}\) | No-slip |
| Mid-plane \(U_\mathrm{max}\) relative error | \(< 2\%\) on a mesh with \(\ge 20\) cells across \(H\) after algebraic convergence | Second-order laminar channel; allows discretisation residual before GCI study |
| Profile \(L_\infty\) relative to \(U_\mathrm{max}\) | \(< 3\%\) on that mesh | Same |

These thresholds apply to a **future OpenFOAM compare** against the analytic profile. They are **not** claimed as measured until `scripts/run_openfoam.sh` has been executed and results recorded under `results/` (gitignored except `results/.gitkeep` notes).

## Software checks (always)

`Poiseuille.*` tests in `test_verification` assert the analytic identities and that `CaseWriter` accepts the case.

## How to run with OpenFOAM (optional)

```bash
# From repo root, with OpenFOAM sourced and icoFoam/simpleFoam available:
./benchmarks/poiseuille-laminar-2d/scripts/run_openfoam.sh /path/to/work/dir
```

Use `benchmark_write_case` and `scripts/run_openfoam.sh` (requires OpenFOAM on PATH). CTest skips with code 2 when solvers are absent.

## Mesh independence

Refine cells across \(H\) (e.g. 20 / 40 / 80) at fixed aspect, extract \(U_\mathrm{max}\) at \(x=L/2\), then:

```bash
python3 benchmarks/common/gci.py --f1 <Umax_fine> --f2 <Umax_mid> --f3 <Umax_coarse> --r21 2 --r32 2
```

Do not mark the case mesh-independent until asymptotic ratio ≈ 1 and GCI₁₂ meets the a-priori band you record with the run.
