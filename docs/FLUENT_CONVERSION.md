# Conversión Fluent → OpenFOAM

## Modelo de datos intermedio

`FluentMesh` (en `src/interop/fluent/FluentTypes.hpp`) representa una malla
Fluent desacoplada del formato de origen:

- `nodes`: id + posición (m).
- `faces`: id + lista de nodos + zone id + flag de contorno.
- `cells`: id + tipo (tet/hex/pyramid/wedge/prism/polyhedron) + nodos + zone id.
- `zones`: id + nombre + tipo (interior/wall/velocity-inlet/pressure-outlet/...).

## Correspondencias de tipos de zona Fluent → parche OpenFOAM

| Tipo Fluent | Parche OpenFOAM |
|---|---|
| `wall` | `wall` |
| `symmetry` | `symmetry` |
| `axis` | `empty` |
| `interface` | `patch` |
| `velocity-inlet`, `pressure-inlet`, `pressure-outlet`, `outflow`, `mass-flow-inlet` | `patch` |
| otros | `patch` |

## Correspondencias de celdas

| Fluent | OpenFOAM (polyMesh) |
|---|---|
| tetra (4 nodos) | tetra |
| hexa (8 nodos) | hex |
| pyramid (5 nodos) | pyramid |
| wedge (6 nodos) | wedge |
| prism | (no soportado en MVP) |
| polyhedron | (no soportado en MVP) |

## Lo que se pierde en la conversión (MVP)

- Valores de condiciones de contorno (U, p, T, k, ε…).
- Modelos de turbulencia y sus parámetros.
- Propiedades de material más allá de la geometría.
- Configuración numérica (esquemas, tolerancias, solvers lineales).
- Campos de resultados (no se importan a OpenFOAM; se visualizan vía VTK).

## Validación

Tras la conversión, ejecutar `checkMesh` sobre el caso generado y revisar el
informe de compatibilidad (`CompatibilityReport`) para zonas y celdas no
mapeadas.
