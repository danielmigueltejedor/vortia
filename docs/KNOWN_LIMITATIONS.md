# Limitaciones conocidas

## Plataforma
- **macOS:** OpenFOAM no tiene binarios nativos. La ejecución CFD requiere contenedor Linux (Docker) o máquina remota. La GUI sí es nativa.
- **Windows:** soporte previsto vía WSL2; no implementado aún.
- **Linux:** plataforma prioritaria para ejecución local.

## Backend
- MVP dirigido a OpenFOAM ESI (solvers `icoFoam`/`simpleFoam`). Foundation ≥10 requiere `foamRun -solver incompressibleFluid`; el mapeo exacto por versión está en la tabla de capacidades y debe verificarse contra la instalación.
- `foamToVTK` o `vtkOpenFOAMReader` se usarán para importar resultados; la disponibilidad del lector VTK varía según la build de VTK.

## Funcional
- Solo dominios rectangulares + importación STL (sin operaciones CAD todavía).
- Solo mallas `blockMesh` (sin snappyHexMesh todavía).
- Solo flujo laminar incompresible, newtoniano, estacionario/transitorio básico.
- Sin MPI todavía.
- Sin STEP/IGES todavía (OCCT integrado más adelante).

## Datos
- Los tests usan solvers falsos (scripts) para probar orquestación; eso NO es validación CFD.
