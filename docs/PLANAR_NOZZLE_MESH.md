# Preliminary planar nozzle mesh export

The CAD sketch now offers **Export planar OpenFOAM mesh** for an **unmodified
parametric nozzle profile**. Generates a conforming multi-block structured
\`blockMeshDict\` with named \`inlet\`, \`outlet\`, \`nozzleWall\`, \`axis\`
and \`frontAndBack\` faces. Every axial station has shared vertices with
its neighbors; fluid spans axis→wall and front/back use one \`empty\` cell.

The generator uses millimetre coordinates with \`convertToMeters 0.001\`.
The throat station is explicit. Edit axial segment counts, radial cell count,
grading and thickness from the export dialog. Inlet/outlet radii and lengths
come from the active parametrically generated sketch.

## Strong limits
- This is a **2D planar approximation**, NOT rotationally axisymmetric flow.
- It is **not** a general mesher for freely edited polygons, intersections,
  circles, or trimmed surfaces. Modified parametric sketches are rejected.
- No solver configuration or BC fields are exported. Vortia's existing
  incompressible cavity solver must NOT be used for supersonic nozzle analyses.
- The output is the dictionary only; save as \`system/blockMeshDict\` inside
  your own appropriate OpenFOAM case, then run \`blockMesh\` and \`checkMesh\`.
- CI tests dictionary structure, dimensions and mesh parameters; it does
  not execute OpenFOAM. User must check mesh diagnostics before CFD use.

Next: true axisymmetric wedge, solver-specific fields for compressible flow,
physical patch assignment and validation against the ANSYS nozzle exercise.
