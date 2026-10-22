# Vortia viewport — visual QA checklist

The CFD viewport uses one persistent, compact scalar bar, a scientific sequential color scale, and preserves camera position when switching fields on the same VTM file.

## macOS ARM64 smoke test

1. Build against Qt 6 and VTK with GUISupportQt, then launch `vortia`.
2. Open or run the lid-driven cavity case; confirm the legend is readable, does not overlap the domain and appears **once**.
3. Switch `p` → `|U|` → `Ux` repeatedly; the camera should not jump or accumulate legend actors. Observe the selected scalar range changing.
4. Confirm an unavailable field (e.g. `T` in the laminar cavity) cannot display a misleading pressure legend.
5. For the thin XY cavity, check a centered orthographic view; rotate deliberately to inspect it, then press XY or Fit.
6. Move the mouse without pressing buttons; camera should stay still. Repeat with trackpad.
7. Load another VTM case and ensure the initial camera fits the new dataset.
8. Test light and dark themes and a window resize at MacBook Air resolution.

## Limits

This PR improves **presentation**, not mathematical validation. It does not add support for a new OpenFOAM solver or parse scientific units from the field metadata. Verify solver-specific pressure units independently. GUI and macOS rendering must be checked manually before merging.
