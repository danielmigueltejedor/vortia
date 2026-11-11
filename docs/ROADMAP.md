# ROADMAP — Vortia

Leyenda: ✅ completado · 🟡 en curso · ⬜ pendiente · 🧪 validado con backend real

**Versión publicada:** 0.3.0 (instaladores autónomos con OpenFOAM integrado, verificación nativa, k-ω SST, fuerzas). Estado por áreas: `docs/CAPABILITY_MATRIX.md`.

## Dirección

Vortia va hacia una aplicación escrita entera en Rust ([`rust/README.md`](../rust/README.md))
y hacia un motor de dinámica de fluidos propio, construido a partir de OpenFOAM, que
sustituya al OpenFOAM integrado. Es una dirección, no algo hecho: hoy la aplicación que se
publica es la de C++/Qt y calcula con OpenFOAM, y cada paso del motor propio tendrá que
reproducir los casos de verificación actuales antes de reemplazar nada.

## Fase 0 — Investigación y fundamentos
- ✅ Investigación de dependencias, versiones y licencias
- ✅ Arquitectura y ADRs
- ✅ Estructura modular del repositorio
- ✅ CMake + compilación + pruebas (GoogleTest)
- ✅ CI (GitHub Actions)

**Criterio de salida:** proyecto compilable, `ctest` verde. ✅

## Fase 1 — Aplicación de escritorio
- ✅ Ventana Qt: toolbar, explorador de proyecto, panel de propiedades, consola
- ✅ Viewport VTK embebido (rotación/pan/zoom, ejes, tema)
- ✅ Importación STL + visualización
- 🟡 Temas claro/oscuro, i18n en/es scaffold, preferencias persistentes
- ✅ Apertura/guardado de proyectos `.vortia`

**Criterio de salida:** abrir Vortia, crear proyecto, importar STL y navegar en 3D. ✅

## Fase 2 — Primera simulación real (MVP)
- ✅ Dominio rectangular + `blockMeshDict` generado
- ✅ Detección de OpenFOAM (ESI primero, Foundation por capacidades)
- ✅ Escritor de caso laminar incompresible (`icoFoam` ESI / `foamRun` Foundation)
- ✅ Ejecución supervisada (logs, cancelación, residuales)
- ⬜ Importación de resultados y visualización p/U (pendiente conversión foamToVTK)
- 🧪 Verificación: cavidad con tapa móvil vs Ghia et al. (Re=100), relL2 0,35 % (0.3.0)

**Criterio de salida:** flujo completo GUI → caso → solver → visor, verificado contra benchmark. Solo se marcará 🧪 con OpenFOAM real. 🟡

## Fase 3 — Mallado avanzado
- ⬜ snappyHexMesh
- ⬜ Parseo de `checkMesh`

## Fase 4 — Modelos físicos
- 🟡 RANS k-ω SST estacionario con funciones de pared (0.3.0); transitorio avanzado, compresible general y energía pendientes

## Fase 5 — Posproceso avanzado
- ⬜ Streamlines, isosuperficies, cortes, sondas, animaciones, fuerzas y coeficientes

## Fase 6 — Aeroespacial
- ⬜ Perfiles NACA, aerodinámica externa, estudios paramétricos

## Fase 7 — Profesional
- ⬜ MPI, remoto/HPC, automatización Python, plugins

## Fase 8 — Lanzamiento
- 🟡 Empaquetado macOS arm64 y Linux x86_64 con motor integrado (0.3.0); notarización Apple, Intel/Windows y auditoría pendientes
