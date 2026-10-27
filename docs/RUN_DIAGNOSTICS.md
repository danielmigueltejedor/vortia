# Diagnósticos y recuperación de cálculos

La evaluación del cálculo distingue un rechazo numérico, valores no finitos,
errores de malla, herramientas no disponibles, fallos explícitos de memoria y
errores sin causa determinada. Conserva la evidencia técnica y ofrece accesos
a Cálculo, Contornos y Malla, además de exportar el diagnóstico como JSON.

`No positive conservative update after 30 reductions` significa que el motor
agotó sus reintentos sin aceptar una actualización admisible. La presencia de
`Vortia rejected explicit energy` aporta evidencia sobre la etapa rechazada,
pero no identifica una celda, un contorno incorrecto ni un defecto de hardware.
`MPI_ABORT` es el cierre de los procesos paralelos tras el fallo. Estos mensajes
no demuestran por sí solos que todos los estados guardados sean inválidos.

## Preparar un reinicio desde cero

En un proyecto editable, el botón **Preparar reinicio desde cero**:

1. Conserva el caso anterior, sus resultados y su registro.
2. Propone un Courant acústico `max(0.001, min(0.02, Courant_actual/4))`.
   Es una heurística de reintento, no una prueba de estabilidad o convergencia.
3. Prepara una revisión nueva con la geometría y los valores iniciales actuales.
   No copia los resultados de la última iteración al caso nuevo.
4. Mantiene gas, contornos, valores iniciales, número de procesos y criterios
   de convergencia. No aumenta el límite de iteraciones ni relaja tolerancias.
5. Abre Cálculo para que el usuario revise los controles y pulse Ejecutar.
   El botón no ejecuta el solver automáticamente.

Si no se puede preparar la revisión, se restauran los controles y la referencia
al caso anterior. La opción no modifica casos importados; hay que abrir su
proyecto editable. Reanudar un checkpoint continúa siendo una decisión separada.

La guía recomienda verificar Pa absolutos, temperaturas en K, asignaciones y
propiedades del gas, así como calidad y transiciones de malla. Una prueba en
serie sirve para comparar comportamientos; no se presenta como solución.
Si el fallo persiste, puede requerir una corrección del motor experimental.

## Registro y alcance

`vortia-run-diagnostic.json` se guarda atómicamente en el caso, con esquema,
estado, código, resumen, fase, última iteración observada y un extracto acotado
del registro. Un intento posterior sustituye el estado anterior, para evitar
mostrar como actual un fallo antiguo. El registro completo sigue en `log.vortia`.
En casos de versiones anteriores se reconoce el fallo de positividad en el
último paso del registro. El diagnóstico no certifica la validez de un checkpoint
ni la solución física, y no modifica las ecuaciones del solver.

Las pruebas incluyen el registro paralelo proporcionado por el usuario, fallos
sin causa determinada, separación de intentos, persistencia, controles numéricos
y un reinicio aislado real que conserva los archivos del caso fallido. La prueba
de preparación no es una simulación validada de la tobera.
