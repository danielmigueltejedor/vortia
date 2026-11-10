# Vortia

**Engineering the Flow** — aplicación de escritorio libre y profesional para simulación CFD con OpenFOAM.

Vortia aspira a ser una alternativa abierta, moderna y rigurosa a las suites CFD comerciales: geometría → malla → física → ejecución supervisada → posproceso 3D interactivo, sin tocar la terminal.

> Estado: **0.3.0** — experimental. Instaladores autónomos para macOS (Apple Silicon) y Linux (x86_64) con OpenFOAM y mallador integrados. Qué hace y qué no: [`docs/CAPABILITY_MATRIX.md`](docs/CAPABILITY_MATRIX.md) · limitaciones: [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md) · verificación: [`docs/VALIDATION.md`](docs/VALIDATION.md).

## Instalación

| Sistema | Archivo | Requisitos |
|---|---|---|
| macOS | `Vortia-<versión>-macos-arm64.dmg` | Apple Silicon, macOS 14+. Firma ad-hoc sin notarizar: la primera vez, clic derecho → Abrir |
| Linux | `Vortia-<versión>-linux-x86_64.AppImage` | x86_64, glibc 2.31+ (Ubuntu 20.04+, Debian 11+, Fedora, Arch…). Dar permiso de ejecución y abrir |

No hace falta instalar OpenFOAM, MPI, Python ni Gmsh: van dentro. Para
comprobar una instalación: `vortia --verify-engine` (caso real en serie y en
paralelo) y `vortia --verify-mesher`.

## Stack

C++20 · Qt 6 · VTK · OpenFOAM (motor externo, vía procesos) · Open CASCADE (fases posteriores) · CMake · GoogleTest

## Requisitos

| Herramienta | Versión mínima | Notas |
|---|---|---|
| CMake | 3.24 | |
| Compilador C++20 | Clang 15 / GCC 12 / MSVC 2022 | |
| Qt | 6.5 | Widgets, OpenGLWidgets |
| VTK | 9.2 | Con módulo `GUISupportQt` (build Qt6) |
| OpenFOAM | ESI v2412+ o Foundation 11+ | Solo al compilar desde el código; los instaladores lo incluyen |

### macOS (Homebrew)

```sh
brew install cmake ninja qt vtk nlohmann-json hdf5 googletest
# Motor: OpenFOAM.app (gerlero/openfoam-app) o el integrado en el DMG publicado.
```

### Ubuntu 24.04

```sh
sudo apt install cmake ninja-build qt6-base-dev libgl1-mesa-dev
# VTK 9 con Qt6: apt (libvtk9-qt-dev) o build propio si la versión no incluye GUISupportQt
# OpenFOAM ESI: https://develop.openfoam.com/Development/openfoam/-/wikis/precompiled
```

## Compilación

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure
./build/bin/vortia
```

## Documentación

- Arquitectura y ADRs: `docs/ARCHITECTURE.md`
- Investigación técnica y licencias: `docs/architecture/research.md`
- Roadmap: `docs/ROADMAP.md` · Estado: `docs/IMPLEMENTATION_STATUS.md`
- Limitaciones conocidas: `docs/KNOWN_LIMITATIONS.md` · Validación: `docs/VALIDATION.md`

## Licencia

GPL-3.0-or-later (propuesta tras la revisión de compatibilidad de dependencias; ver `docs/architecture/research.md` §8). Vortia es una marca e implementación originales, sin relación con ANSYS, Inc.

## Icono de aplicación

El original vectorial es `resources/icons/vortia.svg`; el PNG se regenera con
`swift packaging/macos/render_icon_svg.swift resources/icons/vortia.svg resources/icons/vortia.png 512`.
El icono oficial de Vortia está en `resources/icons/vortia.png` y se incluye en el
ejecutable con el sistema de recursos de Qt (`:/vortia/app-icon.png`), para que
la ventana y Dock utilicen el mismo diseño.

En macOS, para generar una aplicación `.app` con icono de Finder:

```sh
cmake --build build-macos --target vortia_macos_bundle
open build-macos/Vortia.app
```

El ejecutable de desarrollo `build-macos/bin/vortia` sigue disponible. Para un
paquete distribuible, `packaging/macos/deploy_bundle.py` incorpora Qt/VTK y
firma ad-hoc; el DMG publicado lo construye el workflow `Release packages`.


## Cálculo compresible en varios procesos

En Configuración del flujo → Inicialización y cálculo, Procesos de CPU permite
elegir de 1 a 8 procesos (1 es secuencial). Cambiar este ajuste conserva la malla
y los campos guardados. Pulsa Ejecutar para continuar; Generar malla crea otra
revisión. Los proyectos anteriores mantienen un proceso hasta editar y guardar
esta opción. El valor inicial de proyectos nuevos se adapta al equipo, hasta 8.

La app conserva el motor anterior y prepara la revisión incluida en la app en
solver-backends/<hash>, con su ejecutable independiente en bin/<hash>. La
primera ejecución de una revisión puede compilarla. Los contornos existentes
no se modifican. El cálculo paralelo usa MPI/Scotch dentro de parallel-runs;
reconstruye únicamente el último estado completo al terminar o cancelar. Las
escrituras intermedias por partición y los registros se conservan en esa
carpeta para recuperación. Inclúyela en las copias de seguridad. Si falla la
reconstrucción, el último estado del caso principal permanece intacto.

La aceleración depende del equipo y la malla; más procesos pueden añadir
coste de comunicación. No se han aumentado los pasos de Courant ni relajado
las tolerancias para obtener esta ganancia. El cálculo compresible continúa
siendo experimental: comprueba convergencia, balance de masa, dependencia de
malla y sensibilidad a la partición, especialmente en los campos turbulentos.


En el escritorio, los procesos Open MPI ceden CPU mientras esperan mensajes,
para evitar esperas activas que empeoran mucho el rendimiento si coinciden
varios trabajos. No cambia las ecuaciones ni sus tolerancias. Un valor explícito
de OMPI_MCA_mpi_yield_when_idle en el entorno capturado se conserva. La política
se limita al proceso MPI; no modifica preferencias globales del equipo. Para
obtener el menor tiempo total, evita ejecutar varios cálculos que, sumados,
soliciten más procesos que los núcleos disponibles.
