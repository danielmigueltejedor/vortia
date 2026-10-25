# Validación científica

Distinguimos tres niveles (no intercambiables):

1. **Prueba de software:** unidades, parsers, escritores, procesos (tests unitarios/integración).
2. **Verificación numérica:** comparación con soluciones analíticas o benchmarks (Poiseuille, cavidad Ghia).
3. **Validación física:** comparación con experimentos (fases posteriores).

## Matriz de casos

| # | Caso | Nivel | Estado | Evidencia |
|---|---|---|---|---|
| 1 | Poiseuille (perfil analítico) | Verificación | ⬜ Pendiente (requiere backend) | — |
| 2 | Cavidad tapa móvil Re=100 (Ghia et al. 1982) | Verificación | ⬜ Pendiente | — |
| 3 | Canal con escalón | Verificación | ⬜ Pendiente | — |
| 4 | Perfil aerodinámico | Verificación/Validación | ⬜ Pendiente | — |
| 5 | Sensibilidad a la malla | Verificación | ⬜ Pendiente | — |
| 6 | Tobera práctica 2026 (rhoSimpleFoam, wedge, SST) | Verificación | 🟡 Ejecutado, pendiente comparación | examples/nozzle-practice-2026 |

## Procedimiento por caso
Definición física → hipótesis → parámetros (con unidades) → geometría → malla → modelo → esquemas → solución de referencia → métricas de error (L2/L∞) → límites de validez. Todo bajo `benchmarks/<caso>/` con `README.md` reproducible.

## Registro
*(vacío — se rellenará con resultados reales; prohibido registrar resultados sintéticos como si fueran validación)*


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
