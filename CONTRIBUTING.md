# Contribuir a Vortia

## Principios irrenunciables

1. **Nada de fachadas:** toda funcionalidad tiene comportamiento real probado, o se etiqueta explícitamente como no implementada.
2. **Rigor científico:** ninguna capacidad CFD se anuncia sin verificación/validación documentada en `docs/VALIDATION.md`.
3. **Nunca datos ficticios** en flujos presentados como reales. Los dobles de prueba solo viven en `tests/`.

## Flujo de trabajo

1. Abre un issue describiendo el cambio (o referencia el hito del roadmap).
2. Rama descriptiva (`feature/openfoam-detector`, `fix/stl-ascii-crlf`…).
3. Código con pruebas: unitarias (GoogleTest), de integración y GUI (Qt Test) según corresponda.
4. `ctest` verde y sin regresiones antes del PR.
5. Actualiza los documentos de seguimiento si tu cambio afecta a estado, limitaciones o arquitectura.

## Estilo

- C++20, `clang-format` (config en raíz), warnings activados (`-Wall -Wextra -Wpedantic`).
- Manejo de errores explícito con `vortia::Expected`; excepciones solo para errores de programador.
- RAII siempre; nada de `new`/`delete` crudos en código nuevo.
- API pública documentada (qué hace, pre/postcondiciones, unidades).
- Mensajes de commit en imperativo, en inglés, referenciando issues.

## Pruebas científicas

Los casos de `benchmarks/` deben poder ejecutarse de forma reproducible e incluir solución de referencia y métricas de error.
