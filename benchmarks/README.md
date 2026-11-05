# Vortia scientific benchmarks

This tree holds **code verification** and (later) **physical validation** cases.

| Rule | Meaning |
|---|---|
| No synthetic “validation” | Do not invent reference values or pass CI with fabricated CFD fields. |
| Cite sources | Every `reference/` dataset lists citation, licence/terms, and units. |
| Separate layers | Software tests ≠ numerical verification ≠ experimental validation. |
| Gate expensive runs | OpenFOAM/Gmsh jobs are optional; analytic checks always run in CTest. |

## Layout

```
benchmarks/
  README.md
  poiseuille-laminar-2d/
  lid-driven-cavity-ghia-re100/
```

Each case contains: problem definition, reference data, how to generate the OpenFOAM case via Vortia APIs, compare scripts, and acceptance criteria set **before** solving.

## Always-on CTest

Analytic / reference-table checks live in `tests/unit/test_verification.cpp` (linked to `vortia::verification`). They do **not** require OpenFOAM.

## Optional OpenFOAM execution

See each case’s `scripts/run_openfoam.sh`. Scripts exit non-zero if the backend is missing; they never fabricate residuals or fields.
