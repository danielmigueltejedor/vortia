# Vortia 2D sketch — first usable geometry editor

The **Model → 2D Sketch** ribbon tool opens a dimensioned profile editor.

- Add exact X/Y vertices in **millimetres**.
- Click a segment to see its measured length and **split it at its midpoint**.
- Set an exact length by moving the segment endpoint along its current direction.
- Round a corner between two straight segments using **Round vertex…** and a radius in mm.
- Select the resulting arc and use **Arc radius…** to resize it while retaining both supporting tangent lines.
- Split circular arcs at their arc-length midpoint without replacing them with chords.
- Open or close a path; calculate exact line/arc perimeter and closed area in the core.
- Save/load `.vsketch.json` files with explicit units and versioned schema.
- The canvas displays a grid and dimensions; selecting vertices populates edit fields.

**Important limits:** This is a line/arc editor and measurement tool, not yet
a fully constrained CAD sketcher. Editing a straight segment may change adjoining lengths.
Arc endpoints and their tangent-support vertices cannot be moved by raw coordinate edits;
use the radius editor for an unsplit tangent fillet. Split arcs and arbitrary imported arcs
retain their geometry but cannot currently be resized by that fillet operation.
There are no Bézier constraints, Booleans, profile regions or
axisymmetric wedge mesher yet. A saved sketch **cannot currently be solved**
in OpenFOAM; the nozzle workflow remains incomplete.

Do not advertise this as fully Fluent-compatible. Next milestones: global dimensional
constraints, face loops/Boolean trim, revolve/axisymmetric sector generation, Gmsh
or native meshing, physical boundary tagging, compressible solver and validation.

## QA
- Add points (0,0),(100,0), set first edge to 154 mm; divide and verify halves.
- Create a closed rectangle; check orientation and area.
- Save, reopen and confirm coordinate precision and units.
- Load corrupted JSON, duplicate vertices, excessively large coordinates: reject.
- Test Qt 6 ribbon action and dialog on macOS ARM64.

## Circular-arc persistence

Line-only sketches continue to use schema version 1. Sketches containing arcs use
version 2 with `segmentBulges`, one finite value per segment (including the closing
segment for closed paths). A value of zero is a straight line; a nonzero value is
`tan(signedSweepRadians / 4)`. The supported magnitude is between 1e-8 and 1,
so arcs are at most semicircles. Vertices remain in millimetres.

Loading validates the entire document before replacing the current drawing. Project
saves and project-tab switches retain these values. Earlier Vortia versions reject
version 2 rather than flattening arcs silently. The existing planar nozzle export
rejects paths containing arcs; curved/axisymmetric meshing is still pending.

Manual QA: draw (0,0), (0,100), (100,100), (200,52); round vertex 2 at R20,
change to R40, save a separate project, switch tabs and reopen. These coordinates
are an editor test, **not** an exact reconstruction of the supplied Ansys tutorial.
