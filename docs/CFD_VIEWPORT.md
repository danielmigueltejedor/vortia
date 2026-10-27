# CFD result workspace

The Results ribbon groups file access, field selection, analysis, view controls and image export. Opening a result selects this ribbon and the CFD viewport. A field selector lists scalar arrays actually present in the loaded fluid block, plus speed/components when a three-component velocity array is available. It does not invent missing Mach or turbulence fields.

## Camera navigation

- Pinch on a macOS trackpad to zoom about the pointer; scroll with two fingers to pan.
- Drag with the left mouse button to orbit. Shift or Space with a left drag pans; the middle/right button also pans.
- Camera → Mouse navigation selects Orbit, Pan or Zoom for an ordinary left drag.
- F or a double click fits the view. Camera also offers Side (XY), Top (XZ), Front (YZ) and isometric views and parallel projection.
- Preferences → Sketch and navigation adjusts CFD pinch/zoom and pan sensitivity independently from sketch zoom.

VTK's gesture recognizers are unregistered: Qt owns all navigation. Hover, a stray VTK event, or a drag after focus loss must not change the camera. Camera operations never modify solver fields.

## Saved-field analysis

Sections → Plane section previews a plane normal to X, Y or Z, at a position in metres. Position can be typed or adjusted with the slider. Accept retains the section, Cancel restores the previous section and camera, and Show full domain restores the volume surface. Sections are interpolated visualization surfaces, not new CFD meshes.

Sample → Probe a point interpolates a scalar or velocity component at coordinates in metres and marks points outside the fluid invalid. Sample → Line profile retains the existing plot and CSV export, using the same available-field list. Analyses use the highest-dimensional, largest fluid block in a composite result; this is not a multi-region selector.

Display offers Surface, Wireframe and Cell edges. Save image writes the current viewport and legend to a PNG using an atomic file save. It is an image of the current saved result, not a convergence certificate.

These tools do not add new physical models or establish equivalence with Fluent. The compressible nozzle including its exterior and turbulence still requires solver/convergence and mesh-independence validation.

## Verification

Automated checks cover analytical point/plane values, invalid points and planes, preservation of source arrays, native pinch/pan and drag modes, focus-loss disarming, cancellation restoration, field/action availability across project tabs and running jobs, popup menu commands, and preference persistence. Existing geometry, solver orchestration and Spanish catalogue regressions also run.

Camera changes enqueue a VTK render on the Qt event loop, coalescing an input burst. A Qt repaint alone reuses the previous VTK framebuffer; it does not render the changed camera (see [QVTKOpenGLNativeWidget rendering contract](https://vtk.org/doc/nightly/html/classQVTKOpenGLNativeWidget.html)). Hidden or invalid OpenGL widgets are not rendered by this timer, and showing the viewport requests a fresh frame.

`CfdNavigation.NativeGesturesUpdateDisplayedPixelsWithoutAnExplicitCaptureRender` requires `VORTIA_NATIVE_CFD_NAVIGATION=1` and a native OpenGL window. It compares displayed framebuffer pixels after pinch, pan, wheel and drag, observes actual VTK renders, verifies burst coalescing and checks that idle hover does not render or move the scene. It deliberately avoids `captureImage()`, whose explicit Render would mask this regression. Headless CI still covers camera state and hover/focus safety; native rendered-frame QA is run separately.

See [Axisymmetric results and reports](AXISYMMETRIC_RESULTS.md) for 360° reconstruction, longitudinal views, U vectors, original-domain statistics, histograms and CSV export. These are postprocessing tools and add no solver physics or convergence claim.
