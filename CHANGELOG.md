# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).

## [Unreleased]

Trabajo hacia **0.3.0** — ver `docs/VORTIA_0.3.0.md`.

## [0.2.0] — 2026-11-06

Fundamentos de verificación y validación (milestones A–B del plan 0.2.0). No se afirman resultados CFD nativos de OpenFOAM hasta registrarlos bajo `benchmarks/*/results/`.

### Añadido
- Plan técnico `docs/VORTIA_0.2.0.md` y módulo `vortia::verification` (métricas L2/L∞, Poiseuille, Ghia Re=100, Blasius clásico, CSV/interpolación).
- Árbol `benchmarks/` (Poiseuille 2D, cavidad Ghia Re=100, placa plana Blasius) con referencias citadas.
- CLI `benchmark_write_case`, `benchmarks/common/compare_profiles.py` y harness CTest (smoke de escritura; runs OF con `SKIP_RETURN_CODE` si no hay solver).
- GCI de tres mallas (Roache/Celik): `computeGci` (C++) y `benchmarks/common/gci.py`, con rechazo de oscilatorios y de órdenes no positivos.
- Valoración de balance de flujo volumétrico inlet/outlet (`assessVolumeFluxBalance`).

### Notas
- Empaquetado macOS DMG + Linux AppImage como en 0.1.1 (ad-hoc; sin notarización Apple).
- OpenFOAM sigue siendo motor externo; no va embebido en los instaladores.
- Hitos C–F del plan 0.2.0 (fuerzas/CL·CD, endurecimiento de workflow, rendimiento, RC completo) se continúan en **0.3.0**.

## [0.1.1] — 2026-11-05

### Añadido
- Actualización in-app desde GitHub Releases (banner, Ayuda → Buscar actualizaciones, preferencia al arrancar).
- AppImage Linux (`Vortia-*-linux-x86_64.AppImage`) vía GitHub Actions.
- DMG macOS con artwork de instalador.

### Corregido
- Hallazgos SonarCloud de packaging (pin de acción micromamba, curl solo HTTPS).

## [0.1.0] — 2026-11-05

### Añadido
- Estructura inicial del repositorio y documentación fundacional.
- Stack implementado: C++20, Qt 6.12, VTK 9.7, OpenFOAM (ESI primero), CMake, GoogleTest.
- Núcleo: unidades dimensionales, `Expected<T>`, logger, ejecutor de procesos seguro, formato de proyecto `.vortia`.
- Geometría: importador STL ASCII/binario, dominio rectangular.
- Malla: generador de `blockMeshDict`.
- Física: material, condiciones de contorno, validación, serialización JSON del setup.
- Solver OpenFOAM: detección, tabla de capacidades, captura de entorno, escritor de casos, parser de logs, orquestador de ejecución.
- Visualización: viewport VTK embebido en Qt (STL, contorno de dominio, ejes, tema).
- Aplicación Qt: ventana principal, explorador de proyecto, panel de propiedades, consola, temas, i18n scaffold, preferencias.
- CI: GitHub Actions (Linux core + sanitizers, macOS con UI).
- 101 tests unitarios/integración + test de orquestación con backend falso.

### Notas
- La validación numérica real (cavidad Ghia Re=100) requiere OpenFOAM instalado y está pendiente en macOS (sin binarios nativos).
