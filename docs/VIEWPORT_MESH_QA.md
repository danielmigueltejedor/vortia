# Vortia CFD viewport — color bars and mesh-resolution QA

## Summary
The reference lid-driven-cavity setup now starts at **48 x 48 x 1** cells (2,304 CFD cells) rather than 20 x 20 x 1 (400 cells). This increases the simulation resolution, not merely the apparent color smoothness. Existing saved projects retain their configured mesh until edited. Use **Mesh** in the Project Explorer to change cell counts.

To display cell-centered OpenFOAM fields more continuously, the viewport averages them to VTK point data **for rendering only** when a block has no existing point arrays. This does not improve the numerical solution or make a coarse result scientifically trustworthy. Perform mesh-independence and numerical verification before relying on fine gradients.

## Reproduction/acceptance checklist
1. Start Vortia on macOS ARM64 with Qt 6 and VTK 9.7; run the default cavity, save its result VTM.
2. Select `p`, `U`, `|U|`, `Ux`, `Uy`, `Uz` at least 20 times. **Exactly one legend** must remain visible, its labels must not overlap or cover the geometry.
3. The legend uses a dedicated right-hand area; the camera fits the CFD domain in the larger left area. Verify at 1280x800 and on a Retina screen.
4. `U` is explicitly visualized as **velocity magnitude** with legend `|U| (m/s)`; component fields preserve their sign. Missing fields should not silently display another variable.
5. View camera XY/iso should remain stationary while switching arrays; hover must not rotate the scene.
6. Check both color themes and resizing, including a narrow window.
7. Compare the coarse 20x20 mesh and default 48x48 mesh with the **same** physical conditions; note any change in the solution and solver runtime.
8. Run `checkMesh` and inspect convergence; validate against a known cavity benchmark before claiming scientific accuracy.

## Caveats
VTK interpolation only affects presentation. The default higher-resolution case may take longer, and finer grids can require lower time steps to satisfy stability criteria. For Fluent interoperability and nozzle cases use mesh size appropriate to the physics; the cavity default does not prescribe nozzle resolution.
