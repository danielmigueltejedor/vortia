# macOS camera hover regression — explicit Qt press gate

Prior work filtered mouse moves by `QMouseEvent::buttons()` and
`vtkInteractorStyle::State` independently. On a Mac trackpad VTK's internal
button state may remain engaged after synthetic events. Those checks do not
guarantee that Qt actually recorded a deliberate press.

Vortia now authorizes the VTK style **only** after a Qt left/middle/right
`MouseButtonPress`. On mouse release, leave, lost focus, window deactivation,
no-button move, or hide the authorization is removed. The VTK style rejects
presses and all camera moves without that authorization.

## Required manual macOS validation
- Start a fresh Vortia process; use XY or Fit to start at a known camera.
- Move pointer over the 3D view without clicking for 30 seconds: no camera change.
- Two-finger pan, scroll and pinch without a modifier: no unwanted rotation.
- Deliberate click+drag: expected rotation. Releasing stops it instantly.
- Drag out of the viewport and back: no lingering rotation.
- Switch between p, U, and |U|: no spontaneous rotation.
- Test MacBook Air trackpad and an external mouse if available.
- If it still rotates, log Qt button press/release events and camera ModifiedEvent
  together to identify any non-mouse camera path; do not apply cosmetic fixes.

This regression test exercises the VTK guard but does **not** simulate Apple's
native touch events. GitHub Actions cannot certify the physical trackpad.
