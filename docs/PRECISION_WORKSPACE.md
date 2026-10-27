# Precision workspace

Vortia retains its original application logo in Finder, the Dock and the
workspace. Precision applies to the interface tool icons: neutral ink, thin
rounded symbols, and a restrained teal accent. Tool SVGs are editable resources;
no icon font or network dependency is required. Icons resolve normal, checked
and disabled ink against the current Qt palette, including a live appearance
change.

The left navigation exposes Project, Geometry, Mesh, Physics, Calculation and
Results (Option/Alt + 1–6). Stage labels have no numeric prefixes. The former tall ribbon is a compact contextual bar, hidden when the workspace already contains its tools.
Commands reuse their existing QAction instances, shortcuts and enabled states.
Wide command groups remain accessible by horizontal scrolling. Search and
Preferences have one persistent location at the bottom of the navigation.

Mesh has a central workspace with the existing Gmsh region editor, partition
cuts, global and near-wall sizes, wedge angle and independent region controls.
Configured editable projects can prepare the actual cell preview asynchronously
on entry. The preview does not run CFD. Cached previews retain their camera;
changing geometry, project or flow settings invalidates the corresponding view.
Each preview owns a snapshot of its geometry. Leaving/switching a document or
closing the window cancels the relevant temporary mesher safely.

Global and regional edits update only their owning document and invalidate its
prepared-mesh fingerprint. They are saved through the ordinary project save
operation. Imported case dictionaries remain protected. Solver execution and
flow configuration wait for an active mesh preview to finish or be cancelled.

Geometry opens directly in the drawing editor, without an Edit Sketch launcher.
Mesh controls and the actual regional preview share the central workspace.
Physics has Models and Boundaries tabs in the central area. Calculation places
initialization, solver and convergence controls beside the live monitors. There
is no second setup dialog in the normal workflow. Native menu/search commands
route to these same controls. Edits preserve other configuration sections and
are saved with the owning project. Switching projects rebuilds the editors;
running a job locks their inputs. Imported dictionaries and legacy cavity-case
forms retain their existing protected/specific behavior.

The inspector shows context-specific controls or the selected project's saved
configuration with units. The project explorer and detailed
console remain available in View. The first layout migration preserves the
previous layout bytes under `window/prePrecisionState`; later user layouts are
retained normally.

Preferences add tool labels and automatic initial/stale mesh previews.
The retired stage-number preference is ignored and removed on the next save. Existing density, interface scale and navigation controls still apply.
Light/dark themes use the same surfaces, controls and accent hierarchy. The
experimental-CFD notice and numerical assessments remain visible when relevant.

This release changes the workspace and mesh editing access. It does not add a
new turbulence model or establish Fluent parity, physical validation, mesh
independence, or convergence of the user's nozzle calculation.

Visual references: [SimScale mesh workflow](https://www.simscale.com/docs/tutorials/tutorial-fluid-flow-through-a-valve/)
and [ParaView properties](https://docs.paraview.org/en/v5.13.1/Tutorials/SelfDirectedTutorial/basicUsage.html).
The design inference is to keep navigation short, place controls beside their
working view and expose each setting in one primary location. These references
guide the layout; they do not imply equivalent solver capabilities.

On macOS with Qt 6.9 or later, the document tabs extend into the native titlebar.
The real AppKit close/minimize/fullscreen controls retain an 84-point reserved
area, while blank header space supports window dragging and double-click zoom.
Other platforms and older Qt versions retain their regular window frame.
