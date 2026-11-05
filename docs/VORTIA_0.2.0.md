# Vortia 0.2.0 — Technical diagnosis and plan

**Baseline audited:** `main` at 0.1.1 (tag `v0.1.1`), 2026-11-05.
**Machine gate (UI off):** configure/build + 171 CTest passed (1 Detector native smoke skipped).

This document is the versioned engineering plan for **0.2.0**. It records what exists today, what is missing for scientific credibility, and the milestone path. It does **not** claim validation that has not been executed.

---

## 1. Diagnosis (honest)

### What is real

| Layer | Status |
|---|---|
| Modular C++20 stack (core→geometry→mesh→physics→solver + viz/interop/ui) | Real |
| Laminar incompressible case write (`CaseWriter`) + `icoFoam`/`simpleFoam` orchestration | Real |
| Compressible sketch path (`prepare.py` + experimental `vortiaCentralFoam`) | Real, experimental |
| Log/residual/mass-monitor parsing; fake-backend orchestration tests | Real (software) |
| Engineering section integrals (VTK); HTML/CSV/JSON reports | Real, limited physics |
| Packaging: macOS DMG (local), Linux AppImage (CI), in-app updater | Real with stated limits |
| Classical V&V (`benchmarks/`, Ghia, Poiseuille, Blasius, NACA, GCI) | **Absent before this work** |

### Critical gaps for 0.2.0 acceptance

1. No reproducible `benchmarks/<case>/` with cited references and quantitative compare.
2. “Converged” / finished run ≠ verified solution (docs say so; product lacked the compare layer).
3. No automated mesh-independence / GCI tooling.
4. No wall-force / CL·CD pipeline; section coefficient ≠ aerodynamic coefficients.
5. ROADMAP / IMPLEMENTATION_STATUS lag the compressible + packaging reality.
6. Project schema has no migrations (`schemaVersion == 1` only).
7. macOS DMG not CI’d; notarization unavailable without paid Developer ID.

### Principle carried forward

Software tests prove software. Code verification and physical validation require independent references, documented hypotheses, and recorded errors. Synthetic “validation” results are forbidden.

---

## 2. Acceptance criteria (from product brief)

Not met until:

1. Reproducible builds on supported platforms.
2. Existing tests still pass.
3. ≥3 numerical benchmarks run and compared to verifiable references.
4. ≥1 experimental comparison when a compatible case exists.
5. Mesh-independence studies for selected cases.
6. Explicit convergence and conservation assessment.
7. Full UI CFD workflow still functional.
8. Reliable case recovery.
9. Technical reports with units and provenance.
10. CI/docs aligned with real capabilities.
11. Known regressions documented.
12. Experimental limits labelled.

Quantitative error tolerances are set **per benchmark** before runs, with scientific justification.

---

## 3. Milestones

| ID | Name | Focus |
|---|---|---|
| **A** | Audit & Scientific Foundations | This doc; `benchmarks/` scaffold; verification metrics library; doc sync |
| **B** | Verified CFD | Poiseuille + Ghia cavity (+ later Blasius); conservation/GCI tools |
| **C** | Engineering Workflow | Geometry/mesh/physics/run hardening |
| **D** | Professional Postprocessing | Forces, CL/CD/Cp where fields allow; comparisons; reports |
| **E** | Performance & Reliability | Profiling, MPI scaling evidence, regressions |
| **F** | 0.2.0 RC | Packaging, docs, release criteria checklist |

**First PR (Milestone A):** verification library + Poiseuille/Ghia packages + always-on analytic tests.

**Harness PR (Milestone B start):** `benchmark_write_case`, sample dictionaries, `compare_profiles.py`, CTest write smoke + OpenFOAM-gated runs (`SKIP_RETURN_CODE 2` when solvers are absent). No fabricated CFD results.

**GCI tooling (Milestone B):** `vortia::verification::computeGci` + `benchmarks/common/gci.py` (Roache three-grid). Always-on unit/native tests with manufactured second-order data. Native Poiseuille/Ghia GCI numbers still require a complete OpenFOAM host — not claimed until recorded under `results/`.

---

## 4. Benchmark priority order

1. **Poiseuille laminar 2D** — analytic profile (code verification).
2. **Lid-driven cavity Re=100** — Ghia et al. 1982 (code verification).
3. Mesh refinement / GCI on those two.
4. Compressible nozzle — only after clear separation of isentropic 1D vs viscous multi-D; experimental data when licensed/documented.
5. Blasius; NACA 0012 — later milestones with cited NASA/TMR or equivalent sources.

---

## 5. Non-goals for early 0.2.0 PRs

- Rewriting OpenFOAM adapters wholesale.
- Claiming Fluent parity.
- Expanding RANS catalogues before SA/laminar/Euler paths are verified.
- Cosmetic refactors without V&V benefit.
- Marking results “validated” because a job exited zero.

---

## 6. References policy

Every reference dataset under `benchmarks/*/reference/` must include citation, licence/terms, units, and extraction notes. Prefer primary literature (e.g. Ghia et al., J. Comput. Phys. 1982) over secondary blog tables; values must match the cited table.
