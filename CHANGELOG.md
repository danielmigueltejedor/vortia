# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).

## [Unreleased]

### Añadido
- Plan técnico **Vortia 0.2.0** (`docs/VORTIA_0.2.0.md`) tras auditoría de la línea base 0.1.1.
- Módulo `vortia::verification` (métricas L2/L∞, Poiseuille analítico, tablas Ghia Re=100, CSV/interpolación, sampleDicts).
- Árbol `benchmarks/` para Poiseuille 2D y cavidad Ghia Re=100 (referencias citadas).
- CLI `benchmark_write_case` y `benchmarks/common/compare_profiles.py` (write→run→sample→compare).
- CTest: `test_verification`, smoke de escritura, y benchmarks OF con `SKIP_RETURN_CODE` si no hay solver.

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
