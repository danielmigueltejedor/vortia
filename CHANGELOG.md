# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).

## [Unreleased]

## [0.9.0] — 2026-11-13

Vortia empieza como Vortia Native, con una bienvenida para crear o abrir un
proyecto, y Vortia Native se actualiza sola como Vortia.

### Añadido
- **Bienvenida:** sin proyecto abierto, Vortia muestra Nuevo proyecto, Abrir
  proyecto y los proyectos recientes, como Vortia Native. Vuelve al cerrar el
  último proyecto. Un proyecto abierto desde ella ocupa el lugar del proyecto
  sin título, en lugar de abrirse en otra pestaña.
- **Vortia Native se actualiza sola:** busca versiones nuevas una vez al día y
  desde Vortia Native ▸ Buscar actualizaciones…, descarga su propia imagen de
  disco, comprueba la suma SHA-256 publicada y la firma, y se reinicia con la
  versión nueva. Si la nueva no arranca, vuelve la anterior. Para llegar a esta
  versión hay que descargarla una última vez a mano.

## [0.8.0] — 2026-11-13

Vortia Native se parece por fin a Vortia: las mismas seis etapas, un editor de
geometría de verdad y los resultados a toda la ventana. La aplicación Qt no
cambia.

### Cambiado
- **Vortia Native** (macOS): la barra lateral sigue las etapas de Vortia,
  Proyecto, Geometría, Malla, Física, Cálculo y Resultados, con la etapa en el
  centro y sus ajustes en el inspector. Cada etapa dice cuál es el paso
  siguiente, y el proyecto muestra qué etapas están hechas.
- **Geometría:** un editor del contorno con herramientas para seleccionar,
  añadir puntos y desplazar, zoom, deshacer y rehacer, cotas, coordenadas bajo
  el cursor y un inspector para el vértice o el segmento elegido (posición,
  longitud, curvatura, dividir, borrar). La tobera paramétrica es el punto de
  partida de un proyecto nuevo.
- **Malla y física:** se dibujan sobre el contorno; en física se elige cada
  contorno con un clic y los colores de la leyenda dicen qué es cada uno.
- **Resultados:** el campo ocupa la ventana, con su nombre y unidades, cortes,
  reflejo sobre el eje, sonda, perfil a lo largo del eje y guardar imagen.
- **Bienvenida y menús:** proyectos recientes en la bienvenida y en Archivo, y
  menús Etapas (⌘1–⌘6) y Simulación (Mallar ⌘B, Calcular ⌘R, Detener ⌘.).

## [0.7.1] — 2026-11-13

La actualización desde la app vuelve a funcionar en macOS, y en Linux los menús
tienen su propia fila.

### Corregido
- **Actualización en macOS:** fallaba siempre con «firma de código no válida»,
  porque la comprobación rechazaba los metadatos del Finder que lleva todo
  archivo de un disco de instalación. Ahora comprueba la firma sin ese modo
  estricto y el error dice por qué falla.
- **Actualización en macOS:** desde la 0.7.0 podía descargar el disco de Vortia
  Native en lugar del de Vortia. Ahora toma siempre el suyo.
- **Actualización en Linux y macOS:** ofrece la versión más reciente que tenga
  paquete para tu sistema, y nunca un archivo `.zsync` en lugar del AppImage.

### Cambiado
- **Menús en Linux:** Archivo, Simulación, Herramientas, Vista, Resultados y
  Ayuda van en su propia fila encima de las pestañas de proyecto. Si la barra
  superior del escritorio muestra los menús, esa fila desaparece.

### Nota
- En macOS, el botón de actualizar de la 0.7.0 es justo el que fallaba:
  descarga la 0.7.1 a mano una vez. Desde la 0.7.1 la app se actualiza sola.

## [0.7.0] — 2026-11-12

Vortia Native para macOS, el motor de casos sin interfaz y el motor propio en
Rust con importación de mallas. La aplicación Qt y sus resultados de cálculo no
cambian.

### Añadido
- **Vortia Native** (macOS): edición en SwiftUI, cliente de `vortia-engine`.
  Las etapas de un caso en la barra lateral, comprobaciones en vivo en el
  inspector, malla, cálculo y parada en la barra de herramientas, residuales con
  Swift Charts y el resultado como corte coloreado con sus estadísticas y su
  perfil central. Liquid Glass en macOS 26.
- **Resultado en 3D** en Vortia Native: la superficie exterior del dominio,
  coloreada con el campo elegido y dibujada con SceneKit, con órbita y zoom
  desde el trackpad (conmutador Corte / 3D).
- **Flujo compresible con contorno** en Vortia Native: contorno del fluido
  sobre el eje con segmentos coloreados por su tipo de frontera, vértices
  arrastrables, arcos, tobera paramétrica a partir de cinco dimensiones;
  configuración del flujo (tamaños de malla, gas, una frontera por segmento,
  estado inicial, criterios de parada) y tarjeta de convergencia.
- **Análisis de secciones e informe de ingeniería** en Vortia Native: dónde corta
  la sección, el sentido hacia delante, los valores de referencia, las catorce
  métricas y el informe guardado como HTML, CSV o JSON.
- **`vortia-engine`:** el flujo de un caso sin interfaz (proyectos, validación,
  escritor y ejecutor del caso de OpenFOAM con residuales en vivo, análisis de
  resultados VTK, croquis, toberas y flujo compresible), con protocolo JSON por
  línea en stdin/stdout ([`docs/architecture/native-engine.md`](docs/architecture/native-engine.md)).
- **Segundo instalador de macOS:** `Vortia-<versión>-native-macos-arm64.dmg`
  junto al DMG de Qt, con la ventana de instalación de las apps de Nodalia.
- **Motor propio en Rust** (todavía no integrado en la aplicación): solucionadores
  `laplacianFoam`, `scalarTransportFoam`, `icoFoam`, `simpleFoam` (laminar, k-ε y
  k-ω SST) y `rhoSimpleFoam` con k-ω SST compresible y condiciones de presión;
  `blockMesh` propio (graduaciones, aristas curvas, celdas colapsadas, vértices
  con nombre y zonas de celdas) e importación de mallas con `vortia gmshtofoam`,
  `fluenttofoam` y `cgnstofoam` (CGNS en formato ADF, como lo exportan Fluent y
  ANSYS Meshing). Cada paso se valida contra OpenFOAM v1912 sobre los mismos
  archivos ([`docs/architecture/rust-engine.md`](docs/architecture/rust-engine.md)).

### Cambiado
- Los bocetos `.vsketch.json` se leen y escriben fuera del editor, de modo que
  la aplicación Qt y el motor comparten el formato.
- Compila con VTK 9.6, que no tiene `SetRenameArrays`.
- La integración continua corre en runners Blacksmith y añade CodeQL y OpenSSF
  Scorecard.

## [0.6.0] — 2026-11-11

Interfaz con la identidad de Nodalia. No cambia el motor ni los resultados de
cálculo.

### Añadido
- **Islas de Nodalia:** barra lateral, área de trabajo e inspector son paneles
  redondeados sobre el marco de la ventana, con títulos en Sora, iconos en
  negrita y la etapa actual invertida.
- **Menú principal** en la cabecera de la barra lateral, como en las demás apps
  de Nodalia: Preferencias (Ctrl+,), Atajos de teclado (Ctrl+?), Acerca de
  Vortia y Salir. Sin una barra superior que reciba los menús, también lleva los
  menús de la aplicación.
- **Ventana de atajos de teclado**, generada a partir de las acciones reales y
  de las etapas (Alt+1…6).
- **Menús en la barra superior de Nodalix** en Wayland, exportados por D-Bus; en
  ese caso la ventana no repite la barra de menús.
- **Controles de ventana configurables:** Preferencias › Apariencia › Controles
  de ventana › Nodalia o Del sistema (GNOME, KDE…). Se aplica al reiniciar.
- **Acerca de** con el estilo de Nodalia: icono, nombre en Sora, versión y enlaces.

### Cambiado
- Los diálogos y avisos no usan la barra de título del sistema: cabecera de
  Nodalia con el título y el botón de cierre. «Proyecto sin guardar» ofrece
  «Descartar» y «Cancelar» en vez de Sí/No.
- Botones de ventana circulares de 28 px con tinte al pasar el puntero (cian,
  ámbar, coral) y nuevos glifos de maximizar y restaurar.
- El inspector es una sola isla con barra de título propia; acoplado no se
  arrastra y en modo ventana se mueve por su título.
- Avisos (actualización, CFD experimental) como islas con icono, sin la línea de
  caja.
- Menús, listas desplegables y tooltips con esquinas redondeadas; el visor 3D
  respeta las esquinas de su isla.
- Los botones que ya muestran su nombre no lo repiten en un tooltip.
- El icono de la app tiene el mismo tamaño que el de las demás apps en Nodalix
  y GNOME.

### Corregido
- Las islas no dejaban margen con el borde derecho de la ventana.
- Las flechas de los campos numéricos se salían del campo al pasar el puntero.
- El nombre de la app en la barra superior salía como «dev.vortia.vortia».
- Las pestañas del inspector se cortaban.

## [0.5.0] — 2026-11-10

Rediseño visual. No cambia el motor ni los resultados de cálculo.

### Cambiado
- **Nueva interfaz con el Nodalia Design Language (NDL)**, el lenguaje de
  diseño común de las aplicaciones de Nodalia, en lugar del estilo propio
  anterior. Acento cian, neutros fríos, controles rectos de 6 px de radio en
  vez de píldoras, pestañas subrayadas, barra lateral hundida y selección como
  tinte suave. Claro y oscuro. Ver `docs/DESIGN.md`.
- **Los 55 iconos de herramientas se han redibujado** con una sola norma de
  rejilla, grosor y terminaciones. Ver `docs/UI_ICONS.md`.
- Los campos de valor ya no ocupan todo el ancho del panel.

### Corregido
- Las casillas sin marcar no mostraban su recuadro.
- El texto de los campos numéricos quedaba pegado al borde izquierdo.
- En modo oscuro, las flechas de desplegables y campos numéricos se dibujaban
  en negro y no se veían.
- El panel de herramientas del croquis mostraba un fondo distinto al del panel
  que lo contiene.
- La barra lateral quedaba unos píxeles más baja y más corta que los paneles
  de su derecha; ahora empieza y termina a su misma altura.
- El visor 3D usaba un fondo fijo ajeno al tema; ahora toma la superficie
  hundida y el color de texto del tema.

### Para desarrollo
- La densidad «Compacta» de Preferencias ahora usa la densidad compacta de NDL
  (controles de 24 px). `VORTIA_DENSITY=compact|regular|comfortable` la
  cambia para una ejecución. Los tokens y la hoja base de `resources/ndl/` se generan en
  `nodalia-design` y se copian sin editar.

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
