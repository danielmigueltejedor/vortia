# Limitaciones conocidas

Actualizado: 2026-10-27. El estado del MVP inicial ha quedado superado por las
herramientas posteriores; estas son las restricciones relevantes actuales.

- El flujo compresible dibujado usa un motor experimental basado en OpenFOAM
  v2606. La aplicación puede detectar y usar el runtime local de este Mac;
  ejecutar en otras instalaciones depende de su compatibilidad real.
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
