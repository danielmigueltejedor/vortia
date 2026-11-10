# Matriz de capacidades — Vortia 0.3.0

Qué hace Vortia hoy frente a las grandes áreas de un solver CFD industrial
(ANSYS Fluent, STAR-CCM+, OpenFOAM usado a mano). No es una comparación de
resultados: no se ha ejecutado ningún caso contra Fluent y Vortia **no tiene
paridad con Fluent**. Sirve para saber qué se puede hacer, con qué evidencia,
y qué no.

Leyenda de evidencia:

- **Nativa** — ejecutado con OpenFOAM real en CI y comparado con una referencia.
- **Software** — pruebas unitarias/integración del código de Vortia, sin solver.
- **Sin evidencia** — existe en la interfaz o el código, sin comprobación registrada.

| Área | En un solver industrial | Vortia 0.3.0 | Evidencia |
|---|---|---|---|
| Geometría | CAD paramétrico, importación STEP/IGES/Parasolid, reparación | Dominio rectangular; croquis 2D con cotas, líneas y arcos; cuñas axisimétricas; importación STL (visualización) | Software |
| Mallado | Poliedros, prismas de capa límite, mallado automático de volumen, adaptación | `blockMesh` estructurado con gradación; Gmsh 2D/axisimétrico desde croquis (integrado en los instaladores); importación de mallas Fluent `.msh` ASCII | Nativa (cuña y regiones en CI); Software |
| Flujo incompresible laminar | Sí | `icoFoam` transitorio y `simpleFoam` estacionario desde la interfaz | Nativa (Poiseuille, cavidad de Ghia) |
| Turbulencia RANS | k-ε, k-ω SST, RSM, transición, funciones de pared y bajo Reynolds | k-ω SST estacionario con funciones de pared (`simpleFoam`); Spalart–Allmaras solo en el flujo compresible experimental | Nativa (placa plana, frente a correlación; una sola malla) |
| LES / DES | Sí | No | — |
| Compresible | Acoplado y segregado, subsónico a hipersónico | Solver propio experimental para toberas axisimétricas (requiere compilarlo; ver limitaciones) | Sin evidencia de validación física; pruebas de software y de rendimiento |
| Transferencia de calor | Conducción, convección, radiación, conjugada | No (la energía solo existe dentro del solver compresible experimental) | — |
| Multifase | VOF, Euler–Euler, partículas, cavitación | No | — |
| Combustión y química | Sí | No | — |
| Malla móvil, 6-DOF, FSI | Sí | No | — |
| Transitorio | Paso adaptativo, esquemas de 2.º orden, estadísticas | `icoFoam` con Euler y paso fijo o limitado por Courant | Software |
| Cálculo paralelo | MPI en clúster, GPU | MPI local de 1 a 8 procesos en el flujo compresible experimental; el motor integrado se autoprueba con 2 procesos | Nativa (autoprueba serie + 2 procesos en cada instalador) |
| Remoto / HPC | Colas, licencias, clúster | No | — |
| Posproceso | Contornos, vectores, líneas de corriente, isosuperficies, animaciones, informes | Contornos y perfiles en el visor VTK, análisis de secciones, informes HTML/CSV/JSON; fuerzas de pared y coeficientes C_D/C_L/C_p como biblioteca y herramienta de línea de órdenes | Nativa (fuerza de pared en Poiseuille); Software |
| Verificación y validación | Guías y casos del fabricante | GCI de tres mallas, balance de caudal, comparación de perfiles; tres casos nativos en CI | Nativa / Software |
| Interoperabilidad Fluent | — | Detección de formatos, importación `.msh` ASCII, visualización de `.cas.h5`/`.dat.h5` | Software; sin archivos reales de Fluent |

## Qué queda fuera y por qué

- **Paridad con Fluent.** Fluent es una suite comercial con décadas de
  desarrollo y validación. Vortia es una interfaz sobre OpenFOAM mantenida por
  una persona. Además, no hay acceso a Fluent para comparar: cualquier
  afirmación de paridad sería inventada.
- **LES/DES, multifase, combustión, calor conjugado, malla móvil.** OpenFOAM los
  tiene, pero exponerlos exige modelo de datos, interfaz y casos de
  verificación propios para cada uno. Ninguno estaba planificado para 0.3.0.
- **Geometría 3D general y mallado de volumen automático** (`snappyHexMesh`,
  STEP/IGES): pendientes; hoy el trabajo real es 2D, plano o axisimétrico.
- **Validación física frente a experimentos.** Los casos nativos son
  verificación numérica (solución exacta o tablas publicadas) y una
  comprobación de plausibilidad frente a una correlación. No validan diseños.

## Cómo leer los resultados nativos

Los números registrados están en [`VALIDATION.md`](VALIDATION.md), con la
ejecución de CI de la que salen. Un caso que termina no es un caso verificado:
solo cuentan los que se comparan con su referencia y cumplen la tolerancia
fijada antes de ejecutar.
