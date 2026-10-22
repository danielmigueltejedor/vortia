# Vortia CAD-style ribbon

The former single row of text buttons has been replaced with a compact engineering
ribbon using actual Qt Actions and native icons. This changes only presentation,
not CFD models or calculations.

## Navigation
- **Home**: create/open/save projects; import OpenFOAM, STL and Fluent.
- **Model**: jump to real geometry, mesh, materials, boundary and solver editors.
- **Simulation**: detect OpenFOAM; run/cancel with consistent enabled state.
- **Results**: change physical fields and switch camera orientation.
- **View**: theme and fit-view controls.

The macOS menu bar continues working with keyboard shortcuts. No action has a
fake handler or is advertised as a CAD tool that Vortia has not implemented.

## Manual validation (macOS)
1. Test all 5 tabs at MacBook Air 1280x800 and at Retina scaling. Check
   icon legibility, horizontal clipping, keyboard focus and tooltips.
2. Confirm Save really saves, Run becomes disabled while running, Cancel
   enables only during a run, and toolbar/menu actions stay synchronized.
3. Test Model icons move the Properties panel to the correct editor.
4. Test Results p/U/|U| selection and the 5 camera actions.
5. Test both light and dark palettes. Verify native macOS menu bar works.
6. Resize the window, hide/show docks and restart Vortia.

### Known limits
This is a first ribbon, **not** a CAD modeling feature set. A full CAD UX
needs contextual tools, a command palette, selection modes, a 3D gizmo,
dimension constraints, mesh inspection and Fluent case controls.
