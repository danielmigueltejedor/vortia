# Limitaciones conocidas

Actualizado: 2026-11-10 (0.3.0). Resumen por áreas en
[`CAPABILITY_MATRIX.md`](CAPABILITY_MATRIX.md).

## Instaladores 0.3.0

- **macOS:** solo Apple Silicon (arm64), macOS 14 o posterior. Firma ad-hoc,
  sin notarización de Apple: la primera vez Gatekeeper avisa y hay que abrir
  con clic derecho → Abrir (o permitirlo en Ajustes → Privacidad y seguridad).
  Probado en macOS 15, 26 y 27; macOS 14 es el mínimo que declaran los
  binarios, sin máquina de prueba.
- **Linux:** AppImage x86_64, glibc 2.31 o posterior (Ubuntu 20.04, Debian 11,
  Fedora, Arch, openSUSE Tumbleweed, Rocky 9 y posteriores). Tras descargarla
  hay que darle permiso de ejecución (Propiedades → Permisos, o `chmod +x`):
  es una propiedad del formato AppImage, el navegador lo quita. Necesita FUSE
  (`fusermount`), presente en los escritorios habituales; sin FUSE se puede
  ejecutar con `--appimage-extract-and-run`. El backend gráfico probado es
  X11/XWayland; el plugin Wayland nativo se incluye como reserva sin probar.
- **Motor integrado:** OpenFOAM v2606 en macOS y v2412 en Linux, con MPI y
  mallador (Python + Gmsh). No hace falta instalar OpenFOAM. `vortia
  --verify-engine` ejecuta un caso real en serie y con 2 procesos.
- **Flujo compresible experimental (toberas):** su solver se compila en el
  equipo la primera vez. En macOS requiere las Xcode Command Line Tools. En la
  AppImage de Linux **no está disponible** en 0.3.0: exige OpenFOAM v2606 y un
  compilador, y el motor integrado es v2412 sin cabeceras de desarrollo.
- OpenFOAM no admite espacios en su ruta; Vortia lo resuelve con un enlace
  sin espacios en la caché del usuario cuando hace falta.

## Restricciones generales

- El flujo compresible dibujado usa un motor experimental basado en OpenFOAM
  v2606 (ver arriba su disponibilidad por plataforma).
- El modelo k-ω SST se ofrece para casos estacionarios con funciones de pared.
  Su única comprobación nativa es el arrastre de una placa plana frente a una
  correlación empírica, con una sola malla: no cubre separación, gradientes de
  presión, transición ni calor. Con OpenFOAM Foundation no se ha ejecutado.
- Las fuerzas y coeficientes (C_D, C_L, C_p) están disponibles como biblioteca
  y herramienta `force_report`; todavía no hay un panel en la interfaz.
- Hay croquis 2D con cotas, líneas/arcos y regiones de malla, además de cuñas
  axisimétricas y visualización de revolución. Esto no constituye un modelador
  CAD general de sólidos 3D ni importación STEP/IGES completa.
- Los modelos disponibles y sus contornos deben evaluarse para cada caso.
  No hay equivalencia completa con Fluent: faltan, entre otros, combustión y
  química, multifase general, LES/DES, malla dinámica, seis grados de libertad,
  interacción fluido-estructura y un sistema general de cálculo remoto/HPC.
- Los informes de [análisis de ingeniería](AEROSPACE_ANALYSIS.md) integran
  secciones de campos guardados. No calculan por sí solos fuerzas viscosas de
  pared, momentos completos ni el empuje de un motor con todos sus términos.
- La comparación de variantes conserva datos y definiciones. No sustituye un
  estudio formal de independencia de malla, incertidumbre o validación física.
- Los informes con huella de integridad no implementan firmas, aprobaciones de
  ingeniería, control de acceso multiusuario ni gestión documental corporativa.
- Los casos importados conservan sus diccionarios; la conversión de formatos
  Fluent no es una reproducción sin pérdidas de todas sus funciones.
- Las comprobaciones numéricas automáticas y los tests de software no validan
  físicamente un diseño. La práctica de la tobera del usuario no debe darse por
  convergida o finalizada por añadir herramientas de postproceso.
