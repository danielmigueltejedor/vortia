# Turbulent flat plate (k-ω SST, wall functions)

| Field | Value |
|---|---|
| Level | Plausibility check against an empirical correlation (not validation) |
| Physics | Steady incompressible RANS, Menter k-ω SST, wall functions |
| Geometry | Plate L = 10 m at y = 0, domain height 2 m, thin z with `empty` |
| Conditions | U∞ = 10 m/s, ν = 1e-5 m²/s → Re_L = 1e7, inlet turbulence intensity 1 % |
| Mesh | 200 × 60 cells, wall-normal grading 100 (first cell centre in the log layer) |
| Reference | Prandtl–Schlichting: C_D = 0.455/(log10 Re_L)^2.58 = 0.00300 |
| Vortia API | `vortia::verification::makeTurbulentPlateCase`, `TurbulenceModel::KOmegaSST` |

## Acceptance (set before the first run)

| Quantity | Threshold | Rationale |
|---|---|---|
| Friction drag coefficient of the plate | within 15 % of the correlation | Correlation scatter (few %), wall-function modelling error and a single mesh |

The 15 % band was fixed before any native run and is deliberately wide: this
case shows that the generated SST case runs and lands in the right range. It
does not establish mesh independence (one mesh), and it does not validate
separated flows, pressure gradients, transition or heat transfer.

## Run

```bash
./benchmarks/turbulent-flat-plate-sst/scripts/run_openfoam.sh /path/to/work/dir
```

Requires an ESI OpenFOAM (`simpleFoam`). The script also prints y+ on the plate
so the wall-function range can be judged; y+ is reported, not gated.

## Recorded result (2026-11-10)

OpenFOAM ESI v2412, `simpleFoam`, converged in 375 iterations
([CI run](https://github.com/danielmigueltejedor/vortia/actions/runs/42755218855)):

| Quantity | Value |
|---|---|
| C_D computed | 0.002698 |
| C_D Prandtl–Schlichting | 0.003004 |
| Difference | −10.2 % (inside the 15 % band set beforehand) |
| y+ on the plate | 25–36, mean 27 |

The result is biased low and y+ sits at the lower end of the wall-function
range. A finer study (several meshes, a low-Reynolds mesh) is needed before
drawing anything stronger than "the generated case runs and is in range".
