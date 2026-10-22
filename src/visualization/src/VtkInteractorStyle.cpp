#include "vortia/visualization/VtkInteractorStyle.hpp"

#include <vtkObjectFactory.h>

vtkStandardNewMacro(vortia::VtkInteractorStyle);

vortia::VtkInteractorStyle::VtkInteractorStyle() = default;

void vortia::VtkInteractorStyle::OnLeftButtonDown() {
    buttonDown_ = true;
    Superclass::OnLeftButtonDown();
}
void vortia::VtkInteractorStyle::OnLeftButtonUp() {
    buttonDown_ = false;
    Superclass::OnLeftButtonUp();
}
void vortia::VtkInteractorStyle::OnMiddleButtonDown() {
    buttonDown_ = true;
    Superclass::OnMiddleButtonDown();
}
void vortia::VtkInteractorStyle::OnMiddleButtonUp() {
    buttonDown_ = false;
    Superclass::OnMiddleButtonUp();
}
void vortia::VtkInteractorStyle::OnRightButtonDown() {
    buttonDown_ = true;
    Superclass::OnRightButtonDown();
}
void vortia::VtkInteractorStyle::OnRightButtonUp() {
    buttonDown_ = false;
    Superclass::OnRightButtonUp();
}

void vortia::VtkInteractorStyle::OnMouseMove() {
    // State alone is insufficient: macOS gesture synthesis can leave a stale
    // VTK interaction state after the physical pointer was released.
    if (!buttonDown_ || this->State == VTKIS_NONE)
        return;
    Superclass::OnMouseMove();
}