# Práctica de tobera — Vortia

Caso de referencia: `examples/nozzle-practice-2026/`.

## Geometría

Tobera convergente-divergente axisimétrica, malla wedge (sector 5°, 1 celda
circunferencial). Dimensiones del tutorial original (mm): 100, 52, 154, 578,
entre otras. Las cotas exactas deben verificarse contra el vídeo de
referencia; la malla incluida usa el perfil del tutorial OpenFOAM `nozzleFlow2D`.

## Modelo físico

- Fluido: aire (gas ideal, `hePsiThermo`, `const` transport, `hConst` thermo).
- Energía: activada (`sensibleInternalEnergy`).
- Turbulencia: RAS, k-omega SST.
- Solver: `rhoSimpleFoam` (estacionario, basado en presión para compresible).

## Condiciones de contorno

| Parche | Tipo | Valor |
|---|---|---|
| inlet | totalPressure / totalTemperature | p0 = 1.2e5 Pa (moderado, estable), T0 = 300 K |
| atmosphere (outlet) | fixedValue p | 1.01325e5 Pa |
| walls | noSlip, adiabatic | — |
| axis | empty | — |
| front/back | wedge | — |

**Presiones del tutorial original** (2.58e6 Pa entrada / 3.0e4 Pa salida,
manométricas) se documentan aquí pero la malla/caso por defecto usa una
relación de presiones moderada y estable. Para reproducir los valores del
tutorial, ajustar `0/p` y `0/T` y sintonizar la relajación; el caso de alta
relación puede requerir `rhoPimpleFoam`.

Las presiones manométricas de Fluent se convierten a absolutas sumando la
presión de operación (101325 Pa) antes de introducirlas en OpenFOAM.

## Ejecución

```sh
cd examples/nozzle-practice-2026
source /Volumes/OpenFOAM-v2606/etc/bashrc
./Allrun   # blockMesh + checkMesh + rhoSimpleFoam
foamToVTK  # para visualizar en Vortia/ParaView
```

## Resultados

- `checkMesh`: Mesh OK (non-orthogonality max ~7.2, skewness ~0.33).
- Convergencia: residuales por debajo de los criterios de `fvSolution`.
- Visualización: presión, velocidad, temperatura, Mach (derivado), perfiles.

## Validación

Ver `docs/VALIDATION.md` (matriz de casos) y el benchmark `nozzle-practice-2026`.