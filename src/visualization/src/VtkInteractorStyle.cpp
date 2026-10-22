#include "vortia/visualization/VtkInteractorStyle.hpp"

#include <vtkObjectFactory.h>

vtkStandardNewMacro(vortia::VtkInteractorStyle);

vortia::VtkInteractorStyle::VtkInteractorStyle() = default;

void vortia::VtkInteractorStyle::setQtDragAuthorized(bool authorized) noexcept {
    qtDragAuthorized_ = authorized;
    if (!authorized)
        buttonDown_ = false;
}

bool vortia::VtkInteractorStyle::qtDragAuthorized() const noexcept {
    return qtDragAuthorized_;
}

void vortia::VtkInteractorStyle::OnLeftButtonDown() {
    if (!qtDragAuthorized_)
        return;
    buttonDown_ = true;
    Superclass::OnLeftButtonDown();
}
void vortia::VtkInteractorStyle::OnLeftButtonUp() {
    const bool wasDragging = buttonDown_;
    buttonDown_ = false;
    if (wasDragging)
        Superclass::OnLeftButtonUp();
}
void vortia::VtkInteractorStyle::OnMiddleButtonDown() {
    if (!qtDragAuthorized_)
        return;
    buttonDown_ = true;
    Superclass::OnMiddleButtonDown();
}
void vortia::VtkInteractorStyle::OnMiddleButtonUp() {
    const bool wasDragging = buttonDown_;
    buttonDown_ = false;
    if (wasDragging)
        Superclass::OnMiddleButtonUp();
}
void vortia::VtkInteractorStyle::OnRightButtonDown() {
    if (!qtDragAuthorized_)
        return;
    buttonDown_ = true;
    Superclass::OnRightButtonDown();
}
void vortia::VtkInteractorStyle::OnRightButtonUp() {
    const bool wasDragging = buttonDown_;
    buttonDown_ = false;
    if (wasDragging)
        Superclass::OnRightButtonUp();
}

void vortia::VtkInteractorStyle::OnMouseMove() {
    // State alone is insufficient: macOS gesture synthesis can leave a stale
    // VTK interaction state after the physical pointer was released.
    if (!qtDragAuthorized_ || !buttonDown_ || this->State == VTKIS_NONE)
        return;
    Superclass::OnMouseMove();
}