# Compatibilidad ANSYS Fluent — Vortia

Matriz viva. Una capacidad solo se marca como soportada tras verificarla con
archivos reales (ver `docs/VALIDATION.md`).

## Formatos

| Formato | Extensión | Detección | Importación | Conversión a OpenFOAM | Visualización VTK | Estado |
|---|---|---|---|---|---|---|
| Malla tradicional | `.msh` | cabecera + estructura | ✅ parser ASCII | ✅ conversor poliédrico (tet/hex/pyramid/wedge) | parcial | P0 |
| Malla CFF | `.msh.h5` | magic HDF5 | ⬜ P1 | ⬜ P1 | ✅ vía vtkFLUENTCFFReader | P1 |
| Caso tradicional | `.cas` | cabecera | ⬜ P1 | ⬜ P1 | ⬜ | P1 |
| Caso CFF | `.cas.h5` | magic HDF5 | ✅ metadatos + geometría | ⬜ P1 | ✅ vtkFLUENTCFFReader | P0 (viz) |
| Resultados tradicionales | `.dat` | cabecera | ⬜ P1 | n/a | ⬜ | P1 |
| Resultados CFF | `.dat.h5` | magic HDF5 | ✅ campos | n/a | ✅ vtkFLUENTCFFReader | P0 (viz) |
| Comprimido | `.gz` | magic gzip | detección | ⬜ | ⬜ | P2 |
| Journal | `.jou` | extensión | ⬜ P2 | n/a | n/a | P2 |
| Workbench | `.flprj/.wbpj/.wbpz` | extensión | ⬜ P2 | n/a | n/a | P2 |

## Herramientas disponibles en esta máquina (macOS ARM64)

- `fluentMeshToFoam`, `fluent3DMeshToFoam`, `checkMesh`, `blockMesh`: **no disponibles**
  (OpenFOAM no es nativo en macOS). La conversión directa del MVP usa un
  escritor propio de `polyMesh`; la validación con `checkMesh` queda pendiente
  de un backend Linux/WSL2/Docker.
- `vtkFLUENTCFFReader` (VTK 9.7): disponible ✅
- HDF5 2.2.0: disponible ✅

## Niveles de compatibilidad reportados

- `fully-compatible`: malla tet/hex/pyramid/wedge sin zonas no mapeables.
- `partially-compatible`: hay celdas no soportadas o zonas sin mapeo directo.
- `visualization-only`: solo se puede visualizar (CFF .cas.h5/.dat.h5).
- `needs-configuration`: falta completar parámetros antes de ejecutar.
- `incompatible`: no se puede usar.

## Limitaciones conocidas (MVP)

- Solo se convierten celdas tetraédricas, hexaédricas, piramidales y de cuña.
- Las condiciones de contorno se mapean a tipos de parche (`wall`, `patch`,
  `symmetry`, `empty`); los **valores** no se transfieren.
- No se infiere ningún modelo de turbulencia ni propiedad física.
- La conversión CFF → caso ejecutable de OpenFOAM no está soportada (P1).
- No se ha validado con archivos reales de ANSYS Fluent en esta máquina.
