# ROADMAP — Vortia

Leyenda: ✅ completado · 🟡 en curso · ⬜ pendiente · 🧪 validado con backend real

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
- ⬜ Validación: cavidad con tapa móvil vs Ghia et al. (Re=100) — requiere backend real

**Criterio de salida:** flujo completo GUI → caso → solver → visor, verificado contra benchmark. Solo se marcará 🧪 con OpenFOAM real. 🟡

## Fase 3 — Mallado avanzado
- ⬜ snappyHexMesh
- ⬜ Parseo de `checkMesh`

## Fase 4 — Modelos físicos
- ⬜ Transitorio avanzado, RANS, compresible, energía

## Fase 5 — Posproceso avanzado
- ⬜ Streamlines, isosuperficies, cortes, sondas, animaciones, fuerzas y coeficientes

## Fase 6 — Aeroespacial
- ⬜ Perfiles NACA, aerodinámica externa, estudios paramétricos

## Fase 7 — Profesional
- ⬜ MPI, remoto/HPC, automatización Python, plugins

## Fase 8 — Lanzamiento
- ⬜ Empaquetado multiplataforma, documentación, auditoría
