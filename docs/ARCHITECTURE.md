# Arquitectura de Vortia

Versión del documento: 0.1 (2026-10-22)

## 1. Visión general

Vortia es una aplicación de escritorio C++20/Qt 6 que actúa como **front-end científico de OpenFOAM**. La GUI nunca ejecuta CFD por sí misma: genera casos válidos, los ejecuta mediante procesos externos supervisados e importa los resultados.

```
┌────────────────────────────────────────────────────────────┐
│                        vortia_app (Qt 6)                    │
│  MainWindow │ ProjectExplorer │ Properties │ Console │ VTK  │
├────────────────────────────────────────────────────────────┤
│ vortia_ui (widgets, temas, i18n)                            │
├────────────────────────────────────────────────────────────┤
│ vortia_solver/openfoam │ vortia_mesh │ vortia_geometry      │
│ Detector, CaseWriter,  │ blockMesh   │ STL, dominios        │
│ Runner, LogParser      │ dict gen.   │                      │
├────────────────────────────────────────────────────────────┤
│ vortia_core (C++20 puro): Result, Logger, Units, Process,   │
│ Project (manifest .vortia), Validation                      │
├────────────────────────────────────────────────────────────┤
│ Backend externo: OpenFOAM (blockMesh, icoFoam, checkMesh…)  │
└────────────────────────────────────────────────────────────┘
```

## 2. Reglas de dependencia

- `vortia_core` no depende de Qt ni de ningún otro módulo de Vortia. Es C++20 estándar puro (testeable en cualquier CI).
- `geometry`, `mesh`, `physics`, `solver` dependen solo de `core`.
- `visualization` depende de VTK + `geometry` (nunca de Qt salvo el widget puente, aislado).
- `ui` depende de todo lo anterior; `app` solo ensambla.
- Las dependencias entre módulos son unidireccionales; prohibido introducir ciclos.

## 3. Módulos y responsabilidades

### vortia_core
- `Expected<T,Error>` / `Error` (código + mensaje + contexto): manejo explícito de errores sin excepciones cruzando límites de módulo.
- `Logger` con sumideros registrables (consola GUI, archivo, test).
- `Quantity`/sistema de unidades con dimensiones SI en tiempo de compilación (longitud, tiempo, masa…). Conversiones documentadas; sin unidades implícitas.
- `Process` / `IProcessRunner`: ejecución segura (posix_spawn, argumentos estructurados, captura stdout/stderr, cancelación, timeout). Sin shell intermedio.
- `Project`: manifiesto `.vortia` (JSON versionado, escritura atómica).

### vortia_geometry
- `TriangleMesh` (vértices + caras, validación de índices, bbox).
- `StlReader`: ASCII y binario; tolerante a ficheros reales, estricto ante datos corruptos.
- `BoxDomain`: dominio rectangular (geometría del MVP).

### vortia_mesh
- `BlockMeshDict`: genera `system/blockMeshDict` a partir de `BoxDomain` + parámetros de refinamiento y nombres de parches.
- Fase 3: `SnappyHexMeshDict`, parseo de `checkMesh`.

### vortia_physics
- `FluidMaterial` (modelo de transporte, propiedades con unidades).
- Catálogo de condiciones de contorno por campo (U, p) con validación dimensional.
- `CaseSetup`: agregado físico+numerérico validado antes de escribir el caso.

### vortia_solver (+ openfoam)
- `Installation`: distribución (ESI/Foundation), versión, rutas, entorno (bashrc).
- `Detector`: búsqueda en `WM_PROJECT_DIR`, `VORTIA_OPENFOAM_DIR`, prefijos conocidos; identificación por contenido del `etc/bashrc` (versión con prefijo `v` → ESI) y por herramientas presentes.
- `Capabilities`: tabla por distribución/versión (solvers, utilidades, nombre de `transportProperties`/`physicalProperties`).
- `CaseWriter`: materializa `0/`, `constant/`, `system/` de forma atómica y reproducible. Nunca modifica un caso importado: escribe en copia de trabajo del proyecto.
- `CaseRunner`: ejecuta `blockMesh` + solver vía `IProcessRunner`, con entorno del bashrc de OpenFOAM cargado de forma controlada.
- `LogParser`: residuales, Courant, tiempos, detección de divergencia (NaN, FATAL).

### vortia_visualization
- Viewport VTK: geometría STL, malla (VTU/legacy), campos escalares/vectoriales.
- `QVTKOpenGLNativeWidget` como puente Qt.

### vortia_ui / vortia_app
- MainWindow con docks (explorador, propiedades, consola), toolbar, temas claro/oscuro, i18n (en/es), persistencia de preferencias (QSettings).
- `JobController`: orquesta tareas largas en hilos; la GUI nunca se bloquea.

## 4. Formato de proyecto `.vortia`

Carpeta-proyecto con manifiesto `project.vortia` (JSON):

```json
{
  "schemaVersion": 1,
  "name": "cavity",
  "created": "2026-10-22T…",
  "backend": {"distribution": "esi", "version": "v2606", "path": "…"},
  "geometry": {"kind": "boxDomain", "parameters": {…}},
  "mesh": {"kind": "blockMesh", "parameters": {…}},
  "physics": {…},
  "case": {"dir": "case", "state": "configured|meshed|solved"}
}
```

Escritura atómica (tmp + rename), validación de esquema, migraciones por `schemaVersion`.

## 5. Concurrencia y procesos

- Toda ejecución externa ocurre en un hilo de trabajo con cola de trabajos; señales Qt para progreso.
- Cancelación: SIGTERM → gracia → SIGKILL; limpieza de ficheros parciales solo con confirmación.
- Nunca se construyen líneas de shell con datos del usuario: argv estructurado; rutas validadas (sin `..` fuera del proyecto).

## 6. Decisiones de arquitectura (ADR resumidos)

| ADR | Decisión | Motivo |
|---|---|---|
| D1 | Backend primario ESI OpenFOAM | Solvers de aplicación clásicos estables |
| D2 | Integración por proceso+archivos | Aislamiento de versiones, licencias, fallos |
| D3 | GPL-3.0-or-later para el código propio | Compatibilidad del ecosistema |
| D4 | Core sin Qt | Testabilidad y futura CLI |
| D5 | VTK como visor embebido (no ParaView externo) | Posproceso interactivo in-app |
| D6 | STL primero, STEP (OCCT) diferido | Reducir superficie del MVP |
| D7 | Casos generados reproducibles y diff-able | Validación científica exige trazabilidad |
