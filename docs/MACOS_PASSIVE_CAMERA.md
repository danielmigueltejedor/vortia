# Root-cause mitigation: passive VTK camera + explicit Qt navigation

Before this change, Vortia forwarded Qt input to a custom
`vtkInteractorStyleTrackballCamera`. Even with mouse-state checks, VTK's Qt
adapter could translate touch/native events or timers into camera movement.
That entire route is now disabled with `vtkInteractorStyleUser`, which has no
default camera transformations (VTK upstream documentation). All mouse, wheel
and gesture events are intercepted in `VtkViewport::event` *before* the QVTK
adapter and only deliberate Qt button drags update `vtkCamera`.

The default QVTK touch-event conversion is also disabled.

### Controls
- Hover cursor: no change.
- Left press + drag: orbit.
- Middle/right press + drag: pan.
- Mouse wheel: zoom.
- Trackpad two-finger scroll with Cmd/Ctrl: zoom.
- Native trackpad rotate/pinch gestures: ignored.
- Leave, release, focus loss, hide: cancel any drag.

### Physical Mac smoke test
1. Launch an actually recompiled Vortia and select XY.
2. Hover over the surface for 30 seconds without clicking.
3. Move two fingers on the trackpad without holding Cmd/Ctrl.
4. Click and drag to rotate, release and hover. Repeat after leaving window.
5. Switch fields and ensure camera/legend remain stable.

Headless CI builds verify that the code compiles; they cannot guarantee
actual Mac trackpad event behavior. CFD numerical logic is unchanged.
