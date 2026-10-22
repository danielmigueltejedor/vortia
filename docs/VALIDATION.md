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
