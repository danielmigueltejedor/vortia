# Configuración de Vortia en macOS (Apple Silicon)

## 1. Requisitos

- macOS 14+ en Apple Silicon (M1–M4).
- Homebrew.
- Xcode Command Line Tools: `xcode-select --install`.

## 2. Dependencias de compilación

```sh
brew install cmake ninja qt vtk opencascade googletest nlohmann-json
```

## 3. OpenFOAM (motor CFD)

Vortia no incluye OpenFOAM. En macOS ARM64, la vía recomendada es
**OpenFOAM.app** (gerlero/openfoam-app), que proporciona OpenFOAM v2606
nativo para Apple Silicon:

1. Descargar `openfoam2606-app-arm64.zip` desde las releases de
   https://github.com/gerlero/openfoam-app
2. Copiar `OpenFOAM-v2606.app` a `/Applications`.
3. Lanzar la app una vez para montar el DMG en `/Volumes/OpenFOAM-v2606`.
4. Verificar: `source /Volumes/OpenFOAM-v2606/etc/bashrc && rhoSimpleFoam -help`

Vortia detecta automáticamente `/Volumes/OpenFOAM-v2606`. Si no, configura
`VORTIA_OPENFOAM_DIR=/Volumes/OpenFOAM-v2606`.

## 4. Compilar Vortia

```sh
git clone <vortia>
cd vortia
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build
./build/bin/vortia   # sin UI de Qt? usar build-ui para la GUI
```

Para la GUI completa:

```sh
cmake -S . -B build-ui -G Ninja -DCMAKE_BUILD_TYPE=Release -DVORTIA_BUILD_UI=ON
cmake --build build-ui
./build-ui/bin/vortia
```

## 5. Notas

- La DMG de OpenFOAM debe estar montada para que Vortia detecte el backend.
- Los resultados se visualizan con `foamToVTK` + VTK (incluido en Vortia).
