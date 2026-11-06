# Vortia scientific benchmarks

This tree holds **code verification** and (later) **physical validation** cases.

| Rule | Meaning |
|---|---|
| No synthetic “validation” | Do not invent reference values or pass CI with fabricated CFD fields. |
| Cite sources | Every `reference/` dataset lists citation, licence/terms, and units. |
| Separate layers | Software tests ≠ numerical verification ≠ experimental validation. |
| Gate expensive runs | OpenFOAM/Gmsh jobs are optional; analytic checks always run in CTest. |
| Mesh independence | Use `benchmarks/common/gci.py` / `vortia::verification::computeGci` on three QoI values; do not invent fields. |

## Layout

```
benchmarks/
  README.md
  poiseuille-laminar-2d/
  lid-driven-cavity-ghia-re100/
  blasius-flat-plate/
```

Each case contains: problem definition, reference data, how to generate the OpenFOAM case via Vortia APIs, compare scripts, and acceptance criteria set **before** solving.

## Always-on CTest

Analytic / reference-table checks live in `tests/unit/test_verification.cpp` (linked to `vortia::verification`). They do **not** require OpenFOAM.

## Optional OpenFOAM execution

See each case’s `scripts/run_openfoam.sh`. Scripts exit `2` if solvers/`blockMesh` are missing (CTest `SKIP_RETURN_CODE`), `1` on real compare failure, and never fabricate residuals or fields.

Requires a **complete** OpenFOAM install (ESI `simpleFoam` or Foundation `foamRun`). Ubuntu’s `openfoam` apt meta-package often ships a broken `/usr/share/openfoam` environment and is not a valid verification host.

## Mesh independence (GCI)

After three refined native runs of the same QoI (e.g. \(U_\mathrm{max}\) or a centreline \(L_2\)):

```bash
python3 benchmarks/common/gci.py --f1 <fine> --f2 <mid> --f3 <coarse> --r21 2 --r32 2
```

C++: `vortia::verification::computeGci` (Roache / Celik). Record outputs under `benchmarks/<case>/results/` (gitignored). GCI numbers without the underlying OpenFOAM fields are not a verification claim.
