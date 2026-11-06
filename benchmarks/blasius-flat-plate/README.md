# Blasius laminar flat plate

| Field | Value |
|---|---|
| Level | Code verification (classical BL theory) |
| Physics | Steady 2D incompressible laminar boundary layer |
| Geometry | Plate on \(y=0\), \(0\le x\le L\), freestream above |
| Reference | \(c_f=0.664/\sqrt{\mathrm{Re}_x}\), \(\delta_{99}/x\approx 5/\sqrt{\mathrm{Re}_x}\); \(f'(\eta)\) table |
| Citation | Schlichting, *Boundary-Layer Theory* (classical Blasius) |
| Vortia API | `vortia::verification::makeBlasiusCase`, `localSkinFriction`, `blasiusFPrimeTable` |
| Default | \(U_\infty=1\,\mathrm{m}/\mathrm{s}\), \(\nu=10^{-5}\,\mathrm{m}^2/\mathrm{s}\), \(L=1\,\mathrm{m}\) |

## Hypotheses

- Zero pressure gradient; laminar; Newtonian; thin BL (domain height \(\gg\delta(L)\)).
- 2D via `empty` patches; steady SIMPLE path.
- Compare \(c_f(x)\) and/or a wall-normal \(u/U_\infty\) profile at a station \(x\) via \(\eta\).

## Acceptance (set a priori)

| Metric | Threshold | Rationale |
|---|---|---|
| \(c_f\) at \(x=L/2\) | rel. error \(< 5\%\) vs \(0.664/\sqrt{\mathrm{Re}_x}\) on a wall-resolved mesh | Classical correlation; allows discretisation before GCI |
| Profile \(u/U_\infty(\eta)\) | \(\mathrm{rel}\,L_\infty < 5\%\) vs Blasius table for \(\eta\le 5\) | Same |

**No claim of compliance until native OpenFOAM compare is recorded under `results/`.**

## Software checks (always)

`Blasius.*` unit tests lock \(c_f\), \(\delta_{99}\) (including \(x=0\)), table endpoints, and structural `CaseSetup` validity (`validate()`). Case writing is covered by `benchmark_write_case blasius`.
