# Vortia camera/trackpad interaction test plan

## Why
A Qt/VTK viewport must not rotate or pan just because a user moves the macOS
trackpad cursor. VTK may retain a stale drag state across synthesized gesture
events. The interactor now checks its button-down state in addition to the VTK
style state. Native trackpad gestures are deliberately not treated as mouse
drags.

## Default interaction
- Mouse left-drag: rotate; middle-drag: pan; wheel: zoom.
- Move the pointer with no button held: no camera changes.
- Trackpad pointer motion, two-finger inertial scroll, native gestures:
  **no unintended rotation or zoom**. Use **Cmd/Ctrl + trackpad scrolling**
  for precision zoom; one-finger click-and-drag rotates.
- Use the XY/XZ/YZ/3D/Fit actions to recover a known camera view.

## Manual macOS QA (essential; not covered by headless CI)
1. Open a cavity result and select XY. Move the pointer over the domain for
   30 seconds with no click. Compare the camera position and focal point.
2. Two-finger scroll, pinch and rotate gesture with no modifier; the viewport
   should stay stationary.
3. Click-and-drag with one finger; confirm intentional rotation. Release and
   repeat no-button hover to confirm drag state resets.
4. Hold Cmd and two-finger scroll; check controlled zoom.
5. Switch p/U/|U| repeatedly and ensure the legend and camera remain stable.
6. Test on a physical Apple Silicon Mac trackpad (CI cannot emulate it).

## Local changes
Before checking out this PR, stash uncommitted VtkViewport edits, then inspect
the stash. Do not discard locally edited files.
