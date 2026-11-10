# Vortia 0.3.0 — Continuación tras 0.2.0

**Baseline:** `main` at **0.2.0** (tag `v0.2.0`), 2026-11-06.

**0.2.0 shipped:** scientific foundations and Milestone B tooling (benchmarks tree, analytic verification, three-grid GCI, volume-flux conservation helper, Blasius package). Native OpenFOAM quantitative results are **not** claimed until recorded under `benchmarks/*/results/`.

This document is the engineering plan for **0.3.0**. It continues the unfinished 0.2.0 milestones C–F without rewriting the 0.2.0 diagnosis.

---

## 1. Carry-over from 0.2.0

| ID | Name | Status entering 0.3.0 |
|---|---|---|
| **A** | Audit & Scientific Foundations | Done in 0.2.0 |
| **B** | Verified CFD tooling | Done as *tooling*; native OF compare still open |
| **C** | Engineering Workflow | Pending |
| **D** | Professional Postprocessing | Pending |
| **E** | Performance & Reliability | Pending |
| **F** | Release hardening | Partial (0.2.0 packaging cut); continue |

---

## 2. Goals for 0.3.0

1. **Native verification evidence** — run Poiseuille / Ghia / Blasius on a complete OpenFOAM host; store profiles and GCI tables under `benchmarks/*/results/` with cited tolerances. No fabricated numbers.
2. **Engineering workflow (C)** — harden geometry → mesh → physics → run → recover paths; schema migrations if needed.
3. **Postprocessing (D)** — wall forces and CL/CD/Cp where fields allow; richer reports with units and provenance.
4. **Reliability (E)** — profiling hotspots, MPI scaling notes when available, regression guards.
5. **Release quality (F)** — keep packaging/docs/CI aligned; document remaining Gatekeeper/notarization limits.

### Explicit non-claims until evidence exists

- “Validated against Ghia/Blasius/Poiseuille” requires recorded native results, not only analytic unit tests.
- Finished solver exit ≠ verified solution.

---

## 3. Acceptance sketch

Not met until:

1. Reproducible builds on supported platforms (carry from 0.2.0).
2. Existing tests still pass; new behavior covered.
3. ≥1 benchmark has **native** OF results + quantitative compare committed (or clearly gated CI).
4. Mesh-independence (GCI) applied to at least one recorded study.
5. Conservation assessment demonstrated on a real case flux pair.
6. Known limitations and experimental labels stay honest.

Quantitative tolerances remain **per benchmark**, justified before runs.

---

## 4. Non-goals

- Claiming Fluent parity.
- Expanding RANS catalogues before laminar/verified paths have native evidence.
- Cosmetic refactors without V&V or workflow benefit.
- Marking results “validated” because a job exited zero.

---

## 5. Resultado de 0.3.0 (2026-11-10)

| Criterio (§3) | Estado |
|---|---|
| 1. Builds reproducibles en las plataformas soportadas | Hecho: AppImage y DMG se construyen y se prueban en CI en máquinas limpias |
| 2. Tests existentes pasan; lo nuevo está cubierto | Hecho: 254 tests en `ctest` |
| 3. ≥ 1 benchmark con resultados nativos y comparación | Hecho: Poiseuille, Ghia y placa turbulenta (`docs/VALIDATION.md`) |
| 4. GCI aplicado a un estudio registrado | **No hecho** |
| 5. Conservación demostrada en un par de flujos real | Parcial: balance de cantidad de movimiento de Poiseuille (fuerza de pared = Δp·H·W a 1e-8); el balance de caudal entrada/salida no se ha registrado |
| 6. Limitaciones y etiquetas experimentales honestas | Hecho: `docs/CAPABILITY_MATRIX.md`, `docs/KNOWN_LIMITATIONS.md` |

Hitos: C (flujo de trabajo) parcial — motor y mallador integrados, k-ω SST en
la interfaz; D (posproceso) parcial — fuerzas y coeficientes como biblioteca,
sin panel; E (rendimiento) sin cambios; F (release) hecho salvo notarización.
