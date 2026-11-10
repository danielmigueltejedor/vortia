# Validación científica

Distinguimos tres niveles (no intercambiables):

1. **Prueba de software:** unidades, parsers, escritores, procesos (tests unitarios/integración).
2. **Verificación numérica:** comparación con soluciones analíticas o benchmarks (Poiseuille, cavidad Ghia).
3. **Validación física:** comparación con experimentos (fases posteriores).

## Matriz de casos

| # | Caso | Nivel | Estado | Evidencia |
|---|---|---|---|---|
| 1 | Poiseuille (perfil analítico y fuerza de pared) | Verificación | 🧪 Nativa, cumple | `benchmarks/poiseuille-laminar-2d/`, registro 2026-11-10 |
| 2 | Cavidad tapa móvil Re=100 (Ghia et al. 1982) | Verificación | 🧪 Nativa, cumple | `benchmarks/lid-driven-cavity-ghia-re100/`, registro 2026-11-10 |
| 3 | Placa plana turbulenta, k-ω SST (ley de Prandtl–Schlichting) | Plausibilidad frente a correlación | 🧪 Nativa, dentro de la banda fijada (sesgo −10 %) | `benchmarks/turbulent-flat-plate-sst/`, registro 2026-11-10 |
| 4 | Placa plana laminar (Blasius) | Verificación | 🟡 Caso y referencia; sin ejecución nativa | `benchmarks/blasius-flat-plate/` |
| 5 | Canal con escalón | Verificación | ⬜ Pendiente | — |
| 6 | Perfil aerodinámico | Verificación/Validación | ⬜ Pendiente | — |
| 7 | Sensibilidad a la malla (GCI de tres mallas) | Verificación | 🟡 Herramienta lista; ningún estudio registrado | `benchmarks/common/gci.py` |
| 8 | Tobera práctica 2026 (rhoSimpleFoam, wedge, SST) | Verificación | 🟡 Ejecutado, pendiente comparación | examples/nozzle-practice-2026 |

## Procedimiento por caso
Definición física → hipótesis → parámetros (con unidades) → geometría → malla → modelo → esquemas → solución de referencia → métricas de error (L2/L∞) → límites de validez. Todo bajo `benchmarks/<caso>/` con `README.md` reproducible.

## Registro
Plan técnico 0.2.0: [`docs/VORTIA_0.2.0.md`](VORTIA_0.2.0.md). Las ejecuciones nativas
las hace el job `native-verification` del workflow `Release packages`, con el
mismo motor que va en la AppImage; un caso omitido cuenta como fallo.

Lo que estos registros **no** demuestran: independencia de malla (no hay ningún
GCI registrado), validez fuera de los casos listados, ni validación frente a
experimentos. En la primera ejecución nativa Poiseuille no cumplió (relL2 =
3,1 %) porque se comparó una solución sin converger; se corrigió el presupuesto
de iteraciones y el criterio de convergencia, no la tolerancia.

| Fecha | Caso | Qué se midió | Resultado | Notas |
|---|---|---|---|---|
| 2026-11-05 | Poiseuille / Ghia | Software: identidades analíticas + tablas de referencia + `CaseWriter` acepta casos | PASS (CTest `test_verification`) | **No** es verificación CFD frente a solver |
| 2026-11-05 | Poiseuille / Ghia | Harness write→run→sample→compare | Scripts + CTest (`SKIP` sin OpenFOAM) | Ejecutar `benchmarks/*/scripts/run_openfoam.sh` con OF en PATH para obtener PASS/FAIL real |
| 2026-11-10 | Poiseuille, malla 60×40 | Perfil u(y) en x = L/2 frente al analítico, 81 puntos | relL2 = 7,53e-4, relL∞ = 6,33e-4 (límites 2 % y 3 %). **Cumple** | OpenFOAM ESI v2412 (el motor de la AppImage), `simpleFoam`. Residuos finales Ux = 5,8e-10, p = 7,0e-7. [Ejecución CI](https://github.com/danielmigueltejedor/vortia/actions/runs/42755218855) |
| 2026-11-10 | Poiseuille, malla 60×40 | Fuerza viscosa sobre las dos paredes frente al balance exacto Δp·H·W | 9,9999999e-6 frente a 1e-5 m⁴/s²: error relativo 1e-8 (límite 2 %). **Cumple** | Función `forces` de OpenFOAM leída con `parseForces`. Misma ejecución |
| 2026-11-10 | Cavidad Re = 100, malla 48×48 | u en la línea central vertical frente a Ghia et al. (1982), tabla I, 17 puntos | relL2 = 3,47e-3, relL∞ = 3,89e-3 (límites 5 %). **Cumple** | Convergió en 465 iteraciones. La componente v (tabla II) se muestrea pero aún no se compara. Misma ejecución |
| 2026-11-10 | Placa plana turbulenta Re_L = 1e7, malla 200×60, k-ω SST con funciones de pared | Coeficiente de fricción medio frente a Prandtl–Schlichting | C_D = 0,002698 frente a 0,003004: −10,2 % (banda fijada a priori: 15 %). **Dentro de la banda, con sesgo a la baja** | Convergió en 375 iteraciones. y+ en la placa: 25–36 (media 27), en el extremo bajo del rango de funciones de pared. Una sola malla: sin estudio de independencia. Misma ejecución |
| 2026-11-10 | Motor integrado | Autoprueba: malla, `checkMesh`, `simpleFoam` en serie, `decomposePar`, 2 procesos MPI, reconstrucción, `foamToVTK` | **Pasa** en Ubuntu 20.04/22.04/24.04, Debian 12/13, Fedora, Arch, openSUSE Tumbleweed, Rocky 9 (contenedores limpios), Ubuntu 22.04/24.04 (VM con FUSE) y macOS 15/26 (runners limpios) | Prueba de funcionamiento del motor, no de exactitud |

*(Solo resultados reales; prohibido registrar campos inventados como validación.)*


## Comprobaciones de rendimiento del motor experimental (25/10/2026)

Se comparan copias aisladas del mismo estado de una malla de 12580 celdas,
OpenFOAM v2606 y el mismo bloque 1001–2000, Courant 0,05, tolerancias y
escritura cada 1000. La reconstrucción Minmod emparejada mantiene la función
limitadora y la aritmética originales; la comprobación opcional contrasta cada
cara con OpenFOAM, incluidas caras acopladas MPI. Los campos seriales guardados
p/T/rho/U/nuTilda coinciden a la precisión de escritura de nueve cifras.

Los ensayos MPI de 2/4/8 procesos verifican que cada rango acepta o reintenta
colectivamente los candidatos físicos. Las comparaciones iniciales de 2/4
procesos reproducen p/T/rho a la precisión guardada y U dentro de 1e-8 m/s;
nuTilda presenta diferencias locales de hasta ~3,2 % del máximo global. No
son pruebas de independencia de partición ni de convergencia RANS. El
aproximador meshWave y las operaciones distribuidas también deben estudiarse.

Pruebas de software cubren publicación atómica del último estado, conservación
del estado anterior y carpetas processor preexistentes, fallo de reconstrucción,
recuperación al cancelar, parada de descendientes de procesos y selección del
motor por contenido. El ensayo nativo opcional se activa con
VORTIA_PARALLEL_TEST_CASE y VORTIA_NATIVE_OF_ROOT; añadir
VORTIA_PARALLEL_TEST_CANCEL prueba la recuperación de una escritura completa
al cancelar. Solo deben apuntar a copias aisladas de prueba. Estas comprobaciones
no validan los resultados físicos de la práctica.

La repetición controlada en Apple M4 (4 núcleos de rendimiento y 6 de eficiencia)
midió, con /usr/bin/time y sin otras compilaciones/cálculos simultáneos:

| Motor y procesos | Segundos de reloj, 1000 iteraciones |
|---|---:|
| Anterior, 1 | 35,74 |
| Minmod emparejado, 1 | 30,65 |
| Minmod emparejado, 4 | 14,81 |
| Minmod emparejado, 8 | 13,14 |

La comparación incluye inicio del proceso y escritura de su estado final;
excluye compilación, mallado, descomposición y reconstrucción. La mejora máxima
medida de este bloque es 2,72×; no garantiza el mismo factor en otras mallas o
estados ni establece un tiempo de convergencia. El primer ensayo completo del
orquestador, con compilación del motor y reconstrucción, tardó 23,916 s.

Los monitores de cada ejecución paralela se conservan también en
postProcessing/parallel_run_<id>/; los monitores antiguos no se sobrescriben.
El registro indica la carpeta de trabajo/recuperación correspondiente.


## Esperas MPI en un escritorio compartido (25/10/2026)

Se observó que dos trabajos independientes de ocho procesos en el M4 de diez
núcleos provocaban esperas activas y una ralentización grande. Cada lanzador
cree disponer de todos los núcleos. La política local por defecto pasa a
OMPI_MCA_mpi_yield_when_idle=1 para ceder CPU únicamente en esperas MPI, según
[la documentación oficial de Open MPI](https://docs.open-mpi.org/en/main/launching-apps/scheduling.html#forcing-aggressive-or-degraded-performance-mode).
Se preserva una elección explícita del usuario y no se altera el entorno global.

Ensayo controlado con dos trabajos simultáneos de ocho procesos, 12580 celdas
cada uno, estado inicial 1000, final 1050, Courant 0,05, misma malla, binarias,
tolerancias y escritura final:

| Política de espera | Trabajo A | Trabajo B |
|---|---:|---:|
| Activa, valor 0 | 28,16 s | 28,14 s |
| Ceder CPU, valor 1 | 3,80 s | 3,82 s |

La mejora de ~7,4× solo corresponde a ese escenario con exceso de procesos;
no representa una simulación única. En el ensayo de un solo trabajo de ocho
procesos y 1000 iteraciones se midieron 13,338 s y 13,537 s respectivamente.
Los campos reconstruidos p/T/rho/U/nuTilda coinciden a la precisión guardada
de nueve cifras. Las pruebas de integración verifican la inyección del ajuste
y la conservación de una elección explícita del usuario. Ninguna de estas
mediciones demuestra convergencia física de la tobera.
