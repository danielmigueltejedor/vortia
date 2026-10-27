# Precision workspace

Vortia retains its original application logo in Finder, the Dock and the
workspace. Precision applies to the interface tool icons: neutral ink, thin
rounded symbols, and a restrained teal accent. Tool SVGs are editable resources;
no icon font or network dependency is required. Icons resolve normal, checked
and disabled ink against the current Qt palette, including a live appearance
change.

The left navigation exposes Project, Geometry, Mesh, Physics, Calculation and
Results (Option/Alt + 1–6). The former tall ribbon is a compact contextual bar.
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

The inspector shows the selected project's saved configuration with units,
while Geometry retains its drawing controls. The project explorer and detailed
console remain available in View. The first layout migration preserves the
previous layout bytes under `window/prePrecisionState`; later user layouts are
retained normally.

Preferences add tool labels, stage numbers and automatic initial/stale mesh
previews. Existing density, interface scale and navigation controls still apply.
Light/dark themes use the same surfaces, controls and accent hierarchy. The
experimental-CFD notice and numerical assessments remain visible when relevant.

This release changes the workspace and mesh editing access. It does not add a
new turbulence model or establish Fluent parity, physical validation, mesh
independence, or convergence of the user's nozzle calculation.
