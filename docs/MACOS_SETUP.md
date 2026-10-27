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

La instalación local completa de Vortia puede incluir OpenFOAM v2606 en
`Vortia.app/Contents/Resources/OpenFOAM-v2606`. En ese caso basta con abrir
Vortia: no hace falta instalar OpenFOAM.app, montar un disco ni abrir Terminal.
La imagen incluida conserva su sistema de archivos sensible a mayúsculas,
bibliotecas, código fuente y licencias. Preferencias →
Almacenamiento y OpenFOAM permite seleccionar otro motor; deja esa ruta vacía
para utilizar el integrado.

Para construir un paquete completo en macOS ARM64, la distribución de origen es
**OpenFOAM.app** (gerlero/openfoam-app), que proporciona OpenFOAM v2606
nativo para Apple Silicon:

1. Descargar `openfoam2606-app-arm64.zip` desde las releases de
   https://github.com/gerlero/openfoam-app
2. Copiar `OpenFOAM-v2606.app` a `/Applications`.
3. Lanzar la app una vez para montar el DMG en `/Volumes/OpenFOAM-v2606`.
4. Verificar: `source /Volumes/OpenFOAM-v2606/etc/bashrc && rhoSimpleFoam -help`

Para empaquetar el motor, antes de firmar Vortia:

```sh
python3 packaging/macos/bundle_openfoam.py /Applications/OpenFOAM-v2606.app/Contents/Resources/OpenFOAM-v2606.dmg /ruta/Vortia.app
/ruta/Vortia.app/Contents/MacOS/vortia --verify-engine
```

También se puede definir `VORTIA_BUNDLE_OPENFOAM_IMAGE` al ejecutar el objetivo
`vortia_macos_bundle`. El paquete sin esta opción sigue siendo una compilación
de desarrollo con motor externo, no una distribución autónoma.

Sin motor integrado, Vortia busca instalaciones externas y activa automáticamente
el disco de OpenFOAM.app en `/Applications` o `~/Applications`. Los errores de
activación se conservan. Una ruta explícita en Preferencias no se sustituye por
otra versión de forma silenciosa. `Detector::scan()` sigue siendo una inspección
de archivos sin efectos secundarios; `discover()` activa el motor cuando procede.

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

- Vortia incluye la imagen completa del motor y la activa automáticamente en modo
  de solo lectura en `/Volumes/Vortia-OpenFOAM-v2606`, sin mostrarla en Finder.
  No depende de OpenFOAM.app externo ni de montar un disco manualmente.
- El compilador de los solucionadores experimentales de Vortia aún requiere las
  Xcode Command Line Tools si hay que recompilarlos en este Mac.
- Los resultados se visualizan con `foamToVTK` + VTK (incluido en Vortia).
