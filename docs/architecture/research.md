# Investigación técnica inicial — Vortia

Fecha: 2026-10-22. Estado: vivo (actualizar en cada decisión de dependencias).

## 1. OpenFOAM: distribuciones y versiones

Existen dos distribuciones principales, divergentes desde 2016:

| Aspecto | OpenFOAM Foundation (openfoam.org) | OpenCFD / ESI (openfoam.com) |
|---|---|---|
| Versión vigente (oct 2026) | OpenFOAM 14 (julio 2026) | v2606 (junio 2026) |
| Esquema de versiones | Enteros anuales (10, 11, 12, 13, 14) | YYMM (v2412, v2506, v2512, v2606) |
| Solvers | Desde v10/v11 migró a **módulos** ejecutados con `foamRun -solver <módulo>`; `icoFoam` y otros solvers de aplicación fueron eliminados | Mantiene solvers de aplicación clásicos (`icoFoam`, `simpleFoam`, `pimpleFoam`, `snappyHexMesh`…) |
| Licencia | GPLv3 | GPLv3 |
| macOS | Sin binarios nativos (Docker) | Sin binarios nativos (Docker) |

Fuentes: openfoam.org/release/14, openfoam.org/download, openfoam.com/news/main-news/openfoam-v2606, dpcfd.com/posts/2025/08/openfoam-variants.

**Decisión D1:** el objetivo primario del MVP es la distribución **ESI/OpenCFD** (solvers de aplicación clásicos, estructura de tutoriales estable). La compatibilidad con Foundation se implementa mediante tabla de capacidades por versión (`solver/openfoam/Capabilities`), verificada contra la instalación detectada, nunca asumida.

**Decisión D2:** la integración es **por ejecución de procesos e intercambio de archivos de caso**, sin enlazar bibliotecas de OpenFOAM. Motivos:
- Aislamiento de versiones y de fallos (un crash del solver no tumba la GUI).
- OpenFOAM es GPLv3; ejecutarlo como proceso separado evita ambigüedades de obra derivada en el código propio de Vortia.
- Es el mismo modelo que usan FreeCAD/CfdOF y las plantillas de SU2.

## 2. Estructura de un caso OpenFOAM (común a ambas distribuciones)

```
caso/
├── 0/                    # Campos iniciales (p, U, k, epsilon, nut…)
├── constant/
│   ├── transportProperties   (o physicalProperties en Foundation ≥12)
│   ├── turbulenceProperties
│   └── polyMesh/             # Generada por blockMesh/snappyHexMesh
└── system/
    ├── controlDict           # Solver, tiempos, escritura
    ├── fvSchemes             # Discretización
    ├── fvSolution            # Solvers lineales, SIMPLE/PISO, relajación
    ├── blockMeshDict
    └── decomposeParDict      # Paralelo
```

Notas de compatibilidad relevantes:
- `constant/transportProperties` (ESI y Foundation ≤11) vs `constant/physicalProperties` (Foundation ≥12). El escritor de casos debe parametrizarlo.
- La sintaxis de `blockMeshDict` es compatible en la práctica para bloques hex simples.
- Los nombres de condiciones de contorno (`fixedValue`, `zeroGradient`, `noSlip`…) son estables entre versiones recientes; aun así se validan contra la tabla de capacidades.

## 3. Mallado

- **blockMesh**: mallas hex estructuradas por bloques; disponible en ambas distribuciones. Objetivo del MVP para dominios rectangulares.
- **snappyHexMesh**: mallado hex-dominante no estructurado sobre STL (castellated → snap → layers). Fase 3.
- **cfMesh** (GPL, de CfdOF) y **Netgen/Gmsh** (LGPL/GPL): candidatos futuros; Gmsh puede exportar a OpenFOAM vía `gmshToFoam`.
- Métricas de calidad: `checkMesh` reporta non-orthogonality, skewness, aspect ratio, volúmenes negativos. Vortia parseará su salida en lugar de reimplementar las métricas.

## 4. Visualización y CAD

- **VTK 9.x** (licencia BSD): `QVTKOpenGLNativeWidget` para integración con Qt Widgets; lectores de STL (`vtkSTLReader`), de mallas OpenFOAM vía `vtkOpenFOAMReader` (módulo IO/Geometry) — *pendiente de verificación en la build de Homebrew; plan B: convertidor propio a VTU/vía `foamToVTK` (utilidad de OpenFOAM)*.
- **Open CASCADE 7.8/7.9** (LGPL-2.1 con excepción): importación STEP/IGES (Fase 3+). Homebrew: `opencascade`.
- Homebrew `vtk` (≥9.2, ago 2023) se compila contra **Qt 6** y expone `GUISupportQt`. Confirmado en formulae.brew.sh y discourse VTK.

## 5. Qt

- Qt 6 (Homebrew `qt`, 6.9+ en oct 2026). Licencia LGPLv3 (y comercial). Obligaciones: enlazado dinámico, aviso de licencia, posibilidad de re-enlazado. Vortia enlazará dinámicamente.
- Módulos: Widgets, Gui, Core, OpenGLWidgets, Test (para pruebas GUI), LinguistTools (i18n).

## 6. Aplicaciones de referencia (análisis comparativo)

| Aplicación | Lecciones para Vortia |
|---|---|
| FreeCAD + CfdOF | Flujo guiado, escritor de casos OpenFOAM en Python, integración cfMesh; limitado en posproceso propio (delega en ParaView) |
| ParaView | Estándar de posproceso VTK; Vortia incrustará un subconjunto orientado a CFD |
| SALOME | Plataforma modular pesada; evidencia el coste de CAD+malla generalistas → Vortia acota el alcance geométrico |
| SU2 + SU2_GUI | Solver CFD libre alternativo (LGPL); candidato a segundo backend |
| Code_Saturne (GPL) | GUI propia (Salomé-based), gestión de estudios/casos similar al `.vortia` propuesto |

Capacidades clave de ANSYS Fluent (documentación pública) que marcan el horizonte funcional: flujos guiados de mallado (watertight/fault-tolerant), modelos RANS/LES/SAS, multifásico (VOF, Euler, mixture), combustión, CHT, mallas dinámicas, adjoint, parametrización y generación de informes. No se copia UI ni recursos; solo se usa como referencia de capacidades.

## 7. Riesgos identificados

| Riesgo | Impacto | Mitigación |
|---|---|---|
| OpenFOAM no nativo en macOS/Windows | MVP no ejecutable localmente en macOS | Adaptador Docker (Fase 2) + fakes en tests; CI Linux con OpenFOAM real |
| Divergencia Foundation/ESI | Casos inválidos según versión | Tabla de capacidades por versión; detección estricta; nunca asumir nombres |
| VTK/Qt desincronizados en Homebrew | Fallo de enlazado GUI | `find_package` estricto + comprobación de `GUISupportQt` en CMake |
| Coste de OCCT en MVP | Retraso | STL primero; STEP diferido a Fase 3 |
| Licencias | Incompatibilidad | Revisión §8 |

## 8. Licencias (revisión de compatibilidad)

| Componente | Licencia | Obligación principal |
|---|---|---|
| OpenFOAM | GPLv3 | Si se redistribuyen binarios, cumplir GPLv3 (fuentes, avisos). Vortia lo invoca como proceso externo |
| Qt 6 | LGPLv3 / comercial | Enlazado dinámico, aviso, re-enlazable |
| VTK | BSD-3 | Aviso de copyright |
| Open CASCADE | LGPL-2.1 + excepción OCCT | Enlazado dinámico, avisos |
| GoogleTest | BSD-3 | Solo en tests, no se distribuye |

**Decisión D3 (propuesta, revisable antes del release):** código propio de Vortia bajo **GPL-3.0-or-later**. Compatible con todas las dependencias anteriores y coherente con el ecosistema OpenFOAM. Si en el futuro se desea un núcleo reutilizable bajo licencia permisiva, se separará `vortia_core` en un artefacto propio.
