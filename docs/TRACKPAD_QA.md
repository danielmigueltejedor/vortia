# CFD camera and trackpad QA

Qt owns camera navigation; VTK renders the result. Navigation must update both the camera and the displayed framebuffer. Hover alone must remain passive.

## Expected controls

- Trackpad pinch: zoom about the pointer.
- Two-finger scroll: pan. Cmd/Ctrl + scroll: precision zoom. Alt + scroll: orbit.
- Mouse wheel: zoom. Left drag: orbit; Shift/Space + left drag or middle/right drag: pan.
- Camera → Mouse navigation: select Orbit, Pan or Zoom for left drag.
- F or double click: fit. XY/XZ/YZ/isometric actions recover standard orientations.

## Native rendered-frame regression

With a real OpenGL window (macOS uses `QT_QPA_PLATFORM=cocoa`), run `test_workspace --gtest_filter=CfdNavigation.*` with `VORTIA_NATIVE_CFD_NAVIGATION=1`. Optionally set `VORTIA_UI_ARTIFACT_DIR` for framebuffer PNGs. Synthetic Qt gestures test the actual displayed native OpenGL pixels and render events, rather than only the camera coordinates. They do not emulate physical trackpad hardware.

The regression excludes `captureImage()`: that method forces a render and can conceal a frozen interaction frame. Pinch, pan, wheel and drag must each change the displayed pixels. A burst is coalesced; hover leaves both camera and pixels unchanged. Source CFD arrays remain unchanged.

## Physical trackpad check

1. Open a saved CFD field and select XY.
2. Pinch in/out and confirm that the image changes immediately.
3. Scroll with two fingers and confirm pan without rotation.
4. Drag to orbit, then Shift + drag to pan. Release and move the pointer without clicking: the scene must stay still.
5. Fit, switch fields and repeat. Check that the legend remains readable.
6. Switch project/view tabs and return to CFD 3D; repeat navigation.

Never stop an active calculation or discard unsaved projects to perform UI QA. Save and reopen the updated application when the calculation has finished. Headless camera-state tests alone do not establish visible rendering or physical gesture delivery.
