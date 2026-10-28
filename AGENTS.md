# Agent instructions — vortia

## Repository-specific deterministic quality contract (2026-10-26)

**Project:** C++20 CMake CFD CAD/editor and OpenFOAM solver app for macOS.

**Engineering requirements:** Protect mesh validity, dimensional correctness, incompressible flow conservation, Fluent mesh interchange and localization. Use deterministic OpenFOAM nozzle benchmarks and CTest.

**Verification commands:** cmake -S . -B build -G Ninja -DVORTIA_BUILD_UI=OFF -DVORTIA_BUILD_TESTS=ON; cmake --build build; ctest --test-dir build --output-on-failure.

Only claim checks passed after executing them. Every feature and fix should add or maintain tests for behavior, boundaries and failure cases. Never disable/weaken tests, suppress warnings or reduce coverage thresholds solely to pass CI. `AGENTS.md`, `QUALITY_GATES.md`, tests and workflows are review-sensitive. Require `quality` in branch protection. Disclose native dependencies and pre-existing CI failures. Preserve existing project rules.
