#pragma once

#include <vtkInteractorStyleTrackballCamera.h>


namespace vortia {

/// Trackball camera style that NEVER rotates/panns/zooms on bare mouse
/// motion: interaction only happens while a button is pressed. This fixes
/// the macOS hover-spin issue caused by spurious MouseMoveEvents.
class VtkInteractorStyle : public vtkInteractorStyleTrackballCamera {
public:
    static VtkInteractorStyle* New();
    vtkTypeMacro(VtkInteractorStyle, vtkInteractorStyleTrackballCamera);

    // Only a real Qt press in VtkViewport may authorize VTK camera changes.
    void setQtDragAuthorized(bool authorized) noexcept;
    [[nodiscard]] bool qtDragAuthorized() const noexcept;

    void OnMouseMove() override;
    void OnLeftButtonDown() override;
    void OnLeftButtonUp() override;
    void OnMiddleButtonDown() override;
    void OnMiddleButtonUp() override;
    void OnRightButtonDown() override;
    void OnRightButtonUp() override;

protected:
    VtkInteractorStyle();

private:
    bool buttonDown_{false};
    bool qtDragAuthorized_{false};
};

} // namespace vortia
