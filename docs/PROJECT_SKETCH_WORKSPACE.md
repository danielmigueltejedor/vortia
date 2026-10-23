# Vortia central sketch workspace

Vortia now exposes **project document tabs** and **editor tabs** separately:
- Project tabs: independent OpenFOAM setup, case/result state and 2D sketch,
  with unsaved-change indicators and guarded switching during a solver run.
- Editor tabs: **3D CFD** viewport and **2D Sketch** (live interactive canvas
  with mouse drawing, rectangles, circles, dimensioned nozzle templates,
  split segments and editable lengths).

The 2D sketch is saved automatically to
\`<project>/geometry/working-sketch.vsketch.json\` when the Vortia project
is saved, and restored when that project is reopened. The explicit Load/Save
Sketch buttons remain available for importing/exporting a standalone profile.

**Crucial limitation:** these 2D profiles are NOT yet connected to a curved,
axisymmetric OpenFOAM mesher or a compressible solver. The 3D CFD view still
shows the editable rectangular cavity domain for the actual solver.

## Mac QA
1. File → New Project: create a new tab, open 2D Sketch and draw a profile.
2. File → New Project: create another tab, draw a different nozzle.
3. Switch project tabs, verify profiles and CFD settings stay separate.
4. Save both projects to different folders, quit and reopen each.
5. Verify geometry/working-sketch.vsketch.json exists and reopens.
6. Try a malformed sketch file and confirm a warning, not data corruption.
7. Start a solver and verify project tabs are disabled until it finishes.
8. Try closing dirty project tabs and canceling the unsaved-changes prompt.
