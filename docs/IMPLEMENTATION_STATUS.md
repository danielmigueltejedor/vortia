# Estado de implementación

Actualizado: 2026-11-10 (release 0.3.0)

## Implementado (con pruebas)

**0.3.0 — Instaladores autónomos y verificación nativa**
- AppImage y DMG con OpenFOAM, Open MPI, Python y Gmsh integrados; autopruebas
  `--verify-startup`, `--verify-engine`, `--verify-mesher` ejecutadas en CI en
  nueve distribuciones limpias, dos VM Ubuntu y dos versiones de macOS.
- Verificación nativa registrada: Poiseuille (perfil y fuerza de pared),
  cavidad de Ghia y placa plana turbulenta (`docs/VALIDATION.md`).
- k-ω SST estacionario con funciones de pared (modelo, validación, JSON,
  escritor de casos, selector en la interfaz).
- Fuerzas de pared y coeficientes C_D/C_L/C_p (`vortia/solver/openfoam/Forces.hpp`,
  herramienta `force_report`); sin panel en la interfaz todavía.
- Corregidos: esquema ausente en `fvSchemes` para `simpleFoam`, iteraciones
  del `controlDict` estacionario, `PWD` de los procesos hijo, herencia del
  entorno de la AppImage.

**0.2.0 — Verificación (unitario / analítico)**
- `vortia::verification`: métricas de perfil, Poiseuille, Ghia Re=100, Blasius clásico, GCI Roache/Celik, balance de flujo volumétrico.
- `benchmarks/` + `benchmark_write_case` + `compare_profiles.py`; CTest de escritura y GCI; runs OpenFOAM nativos siguen opcionales (`SKIP_RETURN_CODE` sin solver).
- **No** se afirman resultados CFD nativos hasta `benchmarks/*/results/` con referencias.

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
- Flujo GUI completo: nuevo/abrir/guardar proyecto, importar STL, importar ANSYS Fluent (.msh/.cas.h5/.dat.h5), editar setup, detectar OpenFOAM, generar caso, ejecutar y cancelar, ver progreso y logs.
- Interoperabilidad Fluent: detector de formato por magia, parser de .msh ASCII, conversor a OpenFOAM polyMesh (tet/hex/pyramid/wedge), lector CFF vía VTK, informe de compatibilidad.

## Pruebas

- 254 tests en `ctest` (unitarios, integración y benchmarks), de los que los nativos se ejecutan en CI con el motor empaquetado.
- Sanitizers (ASan+UBSan) configurados en CI.
- CI: GitHub Actions (Linux core + sanitizers, macOS con UI).

## Parcial / pendiente

- Blasius nativo y un estudio GCI de tres mallas registrado: pendientes.
- Flujo compresible experimental en la AppImage de Linux (requiere v2606 y compilador).
- Panel de fuerzas y coeficientes en la interfaz; comparación de la componente v de Ghia.
- snappyHexMesh, STEP/IGES (OCCT), remoto/HPC, informes PDF.
- Editor completo de condiciones de contorno (MVP: plantilla de cavidad con tapa móvil).
- Internacionalización completa (solo scaffold en/es).
- Notarización Apple Developer ID (DMG sigue ad-hoc).

## Nunca afirmar como hecho

- Ninguna capacidad CFD se considera operativa hasta ejecutar un caso real y compararlo con referencia (Poiseuille, cavidad Ghia Re=100).
- Los tests con backend falso verifican orquestación, no física.
