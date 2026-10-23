# Vortia central sketch workspace

Vortia now exposes **project document tabs** and **editor tabs** separately:
- Project tabs: independent OpenFOAM setup, case/result state and 2D sketch,
  with unsaved-change indicators and guarded switching during a solver run.
- Editor tabs: **3D CFD** viewport and **2D Sketch** (live interactive canvas
  with mouse drawing, rectangles, circles, dimensioned nozzle templates,
  split segments and editable lengths).

The 2D sketch is saved automatically to
`<project>/geometry/working-sketch.vsketch.json` when the Vortia project
is saved, and restored when that project is reopened. The explicit Load/Save
Sketch buttons remain available for importing/exporting a standalone profile.

Opening an already open project focuses its existing tab, preserving unsaved
edits. Invalid saved setup/sketch data aborts opening before changing the active
document; it is not silently replaced by the default cavity or an empty sketch.
Loading legacy sketches without nozzle dimensions resets the parameter defaults
instead of inheriting them from another document. Standalone import asks before
replacing an existing profile. Each unsaved project's generated case uses its own
temporary directory, retained until that document is closed.

New/Open/Close and switching projects remain locked until the solver completion
signal is delivered. Completed workers are joined before another run can start.
Imported STL/CFF visualization actors remain transient and are not saved with
the project. A planar nozzle dictionary export is available separately; it does
not configure the solver used by the 3D CFD workspace.

Spanish catalogs use the actual namespaced Qt class contexts. The resource test
calls the same class `tr()` functions as the application; the workspace test also
checks translated tree labels and renders the bundled vector icons.
Standard confirmation buttons also use the Spanish catalog. Dense profiles
show dimensions only where labels fit, with the selected segment's measurement
always visible; short wall segments and vertex numbers no longer obscure the
nozzle contour. Their exact values remain available by selecting them.

## Automated regression checks
The macOS UI build runs `vortia_workspace` with the Qt offscreen platform. It
checks nozzle dimension dialogs, exact segment edits and JSON round trips,
independent project setups/profiles, save/reopen, duplicate opens, malformed
files, canceling a dirty-tab close, project locks, embedded-editor Escape,
repeated worker starts and Spanish/icon resources. These are UI/state checks,
not CFD validation. Test preferences and project files use temporary folders.

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
