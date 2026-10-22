# Estado de implementación

Actualizado: 2026-10-22

## Implementado (con pruebas)

**Fase 0 — Fundamentos**
- Investigación técnica y decisiones de arquitectura (`docs/architecture/research.md`, `docs/ARCHITECTURE.md`).
- Estructura modular CMake: `vortia_core`, `vortia_geometry`, `vortia_mesh`, `vortia_physics`, `vortia_solver`, `vortia_visualization`, `vortia_ui`, `vortia_app`.
- Sistema de unidades con análisis dimensional en tiempo de compilación (`vortia/core/Units.hpp`).
- `Expected<T,Error>` para manejo explícito de errores.
- Logger thread-safe con sumideros registrables.
- Ejecutor de procesos seguro (posix_spawn, sin shell, captura stdout/stderr, timeout, cancelación, overlay de entorno) — `vortia/core/Process.hpp`.
- Formato de proyecto `.vortia` con escritura atómica y validación de esquema (`vortia/core/Project.hpp`).
- Importador STL ASCII y binario con detección de formato y validación (`vortia/geometry/StlReader.hpp`).
- Dominio rectangular + generación de `blockMeshDict` (`vortia/mesh/BlockMeshDict.hpp`).
- Modelo físico: material, condiciones de contorno, configuración numérica, validación estructural y heurísticas (Courant, Reynolds, compatibilidad solver/esquema) (`vortia/physics/Physics.hpp`).
- Serialización JSON del setup de caso (`vortia/physics/CaseSetupJson.hpp`).
- Adaptador OpenFOAM: detección (probe/scan), tabla de capacidades por distribución/versión, captura de entorno, resolución de ejecutables (`vortia/solver/openfoam/`).
- Escritor de casos OpenFOAM: `controlDict`, `fvSchemes`, `fvSolution`, `blockMeshDict`, `transportProperties`/`physicalProperties`, `0/U`, `0/p`, marcador `.vortia-case`; protección contra sobrescritura de casos ajenos.
- Parser de logs de solver (residuales, Courant, tiempo, detección de divergencia).
- Orquestador de ejecución `CaseRunner` (blockMesh → checkMesh → solver) con log persistente.
- Viewport 3D VTK embebido en Qt (`QVTKOpenGLNativeWidget`): STL, contorno de dominio, ejes, tema.
- Aplicación Qt: ventana principal con toolbar, docks (explorador, propiedades, consola), menús, temas claro/oscuro, i18n scaffold (en/es), persistencia de preferencias (QSettings).
- Flujo GUI completo: nuevo/abrir/guardar proyecto, importar STL, editar setup, detectar OpenFOAM, generar caso, ejecutar y cancelar, ver progreso y logs.

## Pruebas

- 101 tests unitarios/integración (GoogleTest) + 1 test end-to-end de orquestación con backend falso.
- Sanitizers (ASan+UBSan) configurados en CI.
- CI: GitHub Actions (Linux core + sanitizers, macOS con UI).

## Parcial / pendiente

- Validación numérica real: requiere OpenFOAM instalado (Linux/WSL2/Docker). La detección y el escritor han sido probados sobre árboles de directorios y casos generados; la ejecución real está pendiente en esta máquina (macOS sin OpenFOAM nativo).
- snappyHexMesh, STEP/IGES (OCCT), MPI, remoto/HPC, informes PDF.
- Editor completo de condiciones de contorno (MVP: plantilla de cavidad con tapa móvil).
- Internacionalización completa (solo scaffold en/es).
- Empaquetado e instaladores.

## Nunca afirmar como hecho

- Ninguna capacidad CFD se considera operativa hasta ejecutar un caso real y compararlo con referencia (Poiseuille, cavidad Ghia Re=100).
- Los tests con backend falso verifican orquestación, no física.
