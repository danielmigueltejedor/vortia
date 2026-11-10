# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).

## [Unreleased]

## [0.4.1] — 2026-11-10

### Corregido
- **La actualización desde la propia aplicación no se instalaba en macOS**:
  descargaba la versión nueva, se quedaba «cargando» y al reabrir seguía la
  antigua. El cierre para reiniciar podía quedar cancelado (por ejemplo, con
  un proyecto sin guardar) y el ayudante esperaba indefinidamente, sin dejar
  registro. Ahora Vortia avisa antes si hay proyectos sin guardar, se cierra
  de forma no cancelable, el ayudante deja registro, restaura la versión
  anterior si algo falla y el motivo se muestra al volver a abrir.
  **Quien tenga 0.4.0 o anterior debe instalar esta versión a mano una vez.**
- **DMG de macOS**: al abrirse la ventana más ancha quedaba una franja blanca,
  los nombres de los iconos no se leían sobre el fondo oscuro y había que
  redimensionar la ventana para verlo todo. El fondo se ha redibujado en el
  mismo estilo, más grande y nítido en Retina, con una placa clara tras cada
  nombre y una flecha entre los iconos; la ventana es más alta y sin barra
  lateral.

## [0.4.0] — 2026-11-10

Interfaz rediseñada y coherente.

### Corregido
- **La fila de herramientas aparecía y desaparecía según la etapa**: Geometría
  y Física no tenían, y al cambiar de etapa la navegación lateral y el área de
  trabajo subían y bajaban. Ahora todas las etapas tienen su fila, con la
  misma altura, y nada se mueve.
- En Linux no se veían las etiquetas «Coordenada X/Y» del editor de croquis.

### Añadido
- Herramientas propias de **Geometría** en la fila superior: deshacer, rehacer,
  copiar, pegar, eliminar vértice, archivos de croquis y borrar croquis (ya no
  están repartidas entre el panel lateral y un botón suelto bajo el lienzo).
  **Física** tiene accesos a Modelos y Contornos.
- **Lienzo**, lenguaje de diseño exportable (variante Papel): tokens en
  `resources/lienzo/tokens.json`, hoja de estilos Qt con marcadores y variables
  CSS generadas. Guía en `docs/LIENZO.md`.
- **Linux: una sola barra superior.** Los controles de ventana, el menú y las
  pestañas de documento comparten la barra de la aplicación; ya no hay barra de
  título del sistema ni barra de menús aparte. El lado y los botones siguen la
  configuración del escritorio (GNOME, KDE). `VORTIA_NATIVE_TITLEBAR=1` vuelve
  a la barra del sistema.

### Cambiado
- Tema unificado: un solo aspecto de «seleccionado», acento azul, paneles como
  tarjetas, botones en píldora. El esquema claro pasa a ser el predeterminado
  en instalaciones nuevas; quien tuviera elegido el oscuro lo conserva.

## [0.3.0] — 2026-11-10

Instaladores que abren en una máquina limpia, con el motor CFD dentro.

### Corregido
- **Linux: la AppImage no abría en la mayoría de distribuciones.** Causas
  reproducidas en contenedores limpios con la 0.2.1: `AppRun` era un enlace
  simbólico sin rutas de plugins de Qt; faltaban librerías que linuxdeploy
  excluye (`libharfbuzz`, freetype, fontconfig, `libstdc++`…) y que el Qt de
  conda-forge necesita más nuevas que las de Ubuntu 22.04/24.04 o Debian 12;
  el ejecutable exigía glibc 2.38; y `libOpenGL.so.0` no viene instalada en
  Ubuntu/Debian. Ahora la AppImage lleva todas sus dependencias, elige en el
  arranque el `libstdc++` más nuevo entre el suyo y el del sistema, y se
  prueba en Ubuntu 20.04/22.04/24.04, Debian 12/13, Fedora, Arch, openSUSE
  Tumbleweed y Rocky 9 antes de publicarse.
- **macOS: el DMG 0.2.1 declaraba macOS 13 pero exigía macOS 27** (Qt/VTK de
  Homebrew). El DMG se construye ahora en CI con dependencias de conda-forge y
  se prueba en runners limpios.
- Los casos estacionarios (`simpleFoam`) escritos por Vortia abortaban en la
  primera iteración con OpenFOAM real: faltaba un esquema en `fvSchemes`.
- El `controlDict` estacionario ignoraba el número de iteraciones del caso.
- Los programas que lanza Vortia (OpenFOAM, Python) ya no heredan las
  librerías empaquetadas de la aplicación.

### Añadido
- **OpenFOAM integrado** en ambos instaladores (v2606 en macOS, v2412 en
  Linux, con Open MPI) y **mallador integrado** (Python 3.12 + Gmsh 4.15).
  `--verify-engine` ejecuta un caso real en serie y en paralelo;
  `--verify-mesher` genera una malla; `--verify-startup` abre la ventana
  principal y el visor.
- Modelo de turbulencia **k-ω SST** estacionario con funciones de pared,
  seleccionable en el panel del solucionador.
- **Fuerzas de pared y coeficientes** C_D, C_L y C_p (biblioteca y herramienta
  `force_report`).
- **Verificación nativa en CI** con el motor empaquetado: Poiseuille (perfil y
  fuerza de pared), cavidad de Ghia Re = 100 y placa plana turbulenta.
- Si no hay OpenGL 3.2, Vortia reintenta con renderizado por software y, si
  tampoco, lo explica en un diálogo. Un arranque fallido desde el escritorio
  deja un registro en `~/.local/state/vortia/startup.log`.
- Icono nuevo, con original vectorial en `resources/icons/vortia.svg`.
- [`docs/CAPABILITY_MATRIX.md`](docs/CAPABILITY_MATRIX.md): qué hace Vortia y
  qué no frente a un solver industrial.

### Notas
- Sin paridad con ANSYS Fluent; ver la matriz de capacidades.
- macOS: firma ad-hoc, sin notarización. Requiere macOS 14+ en Apple Silicon.
- Linux: glibc 2.31+. El flujo compresible experimental no está disponible en
  la AppImage (ver `docs/KNOWN_LIMITATIONS.md`).
- El DMG pasa de ~55 MB a ~400 MB y la AppImage de ~100 MB a varios cientos
  por el motor integrado.

## [0.2.1] — 2026-11-06

### Corregido
- Actualizador in-app en macOS: las builds públicas van firmadas **ad-hoc** (`TeamIdentifier=not set`). 0.1.1/0.2.0 rechazaban ese caso tras descargar el DMG, así que la app no se sustituía y al reiniciar seguía pidiendo actualizar. Ahora se acepta ad-hoc → ad-hoc con el mismo `Identifier` (`dev.vortia.app`); Developer ID sigue exigiendo el mismo Team ID.

### Notas
- Quien esté en **0.1.1** o **0.2.0** debe instalar este DMG **a mano** una vez (el binario antiguo sigue con el fallo). A partir de 0.2.1 las actualizaciones in-app macOS vuelven a aplicar.

## [0.2.0] — 2026-11-06

Fundamentos de verificación y validación (milestones A–B del plan 0.2.0). No se afirman resultados CFD nativos de OpenFOAM hasta registrarlos bajo `benchmarks/*/results/`.

### Añadido
- Plan técnico `docs/VORTIA_0.2.0.md` y módulo `vortia::verification` (métricas L2/L∞, Poiseuille, Ghia Re=100, Blasius clásico, CSV/interpolación).
- Árbol `benchmarks/` (Poiseuille 2D, cavidad Ghia Re=100, placa plana Blasius) con referencias citadas.
- CLI `benchmark_write_case`, `benchmarks/common/compare_profiles.py` y harness CTest (smoke de escritura; runs OF con `SKIP_RETURN_CODE` si no hay solver).
- GCI de tres mallas (Roache/Celik): `computeGci` (C++) y `benchmarks/common/gci.py`, con rechazo de oscilatorios y de órdenes no positivos.
- Valoración de balance de flujo volumétrico inlet/outlet (`assessVolumeFluxBalance`).

### Notas
- Empaquetado macOS DMG + Linux AppImage como en 0.1.1 (ad-hoc; sin notarización Apple).
- OpenFOAM sigue siendo motor externo; no va embebido en los instaladores.
- Hitos C–F del plan 0.2.0 (fuerzas/CL·CD, endurecimiento de workflow, rendimiento, RC completo) se continúan en **0.3.0**.

## [0.1.1] — 2026-11-05

### Añadido
- Actualización in-app desde GitHub Releases (banner, Ayuda → Buscar actualizaciones, preferencia al arrancar).
- AppImage Linux (`Vortia-*-linux-x86_64.AppImage`) vía GitHub Actions.
- DMG macOS con artwork de instalador.

### Corregido
- Hallazgos SonarCloud de packaging (pin de acción micromamba, curl solo HTTPS).

## [0.1.0] — 2026-11-05

### Añadido
- Estructura inicial del repositorio y documentación fundacional.
- Stack implementado: C++20, Qt 6.12, VTK 9.7, OpenFOAM (ESI primero), CMake, GoogleTest.
- Núcleo: unidades dimensionales, `Expected<T>`, logger, ejecutor de procesos seguro, formato de proyecto `.vortia`.
- Geometría: importador STL ASCII/binario, dominio rectangular.
- Malla: generador de `blockMeshDict`.
- Física: material, condiciones de contorno, validación, serialización JSON del setup.
- Solver OpenFOAM: detección, tabla de capacidades, captura de entorno, escritor de casos, parser de logs, orquestador de ejecución.
- Visualización: viewport VTK embebido en Qt (STL, contorno de dominio, ejes, tema).
- Aplicación Qt: ventana principal, explorador de proyecto, panel de propiedades, consola, temas, i18n scaffold, preferencias.
- CI: GitHub Actions (Linux core + sanitizers, macOS con UI).
- 101 tests unitarios/integración + test de orquestación con backend falso.

### Notas
- La validación numérica real (cavidad Ghia Re=100) requiere OpenFOAM instalado y está pendiente en macOS (sin binarios nativos).
