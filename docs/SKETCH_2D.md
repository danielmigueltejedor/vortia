# Vortia 2D sketch — first usable geometry editor

The **Model → 2D Sketch** ribbon tool opens a dimensioned profile editor.

- Add exact X/Y vertices in **millimetres**.
- Click a segment to see its measured length and **split it at its midpoint**.
- Set an exact length by moving the segment endpoint along its current direction.
- Open or close a polyline; calculate area for a closed profile in the core.
- Save/load `.vsketch.json` files with explicit units and versioned schema.
- The canvas displays a grid and dimensions; selecting vertices populates edit fields.

**Important limits:** This is a polyline editor and measurement tool, not yet
a fully constrained CAD sketcher. Editing a segment may change adjoining lengths.
There are no curved segments, Bézier constraints, Booleans, profile regions or
axisymmetric wedge mesher yet. A saved sketch **cannot currently be solved**
in OpenFOAM; the nozzle workflow remains incomplete.

Do not advertise this as fully Fluent-compatible. Next milestones: arc and tangent
constraints, face loops/Boolean trim, revolve/axisymmetric sector generation, Gmsh
or native meshing, physical boundary tagging, compressible solver and validation.

## QA
- Add points (0,0),(100,0), set first edge to 154 mm; divide and verify halves.
- Create a closed rectangle; check orientation and area.
- Save, reopen and confirm coordinate precision and units.
- Load corrupted JSON, duplicate vertices, excessively large coordinates: reject.
- Test Qt 6 ribbon action and dialog on macOS ARM64.
