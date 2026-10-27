#pragma once
#include <vtkSmartPointer.h>
class vtkDataObject;
class vtkUnstructuredGrid;
namespace vortia {
// A visual reconstruction from the meridional field, never a solver mesh.
// Rotates vector/tensor components, merges the axis and closes the angular seam.
vtkSmartPointer<vtkUnstructuredGrid> revolveAxisymmetricField(vtkDataObject* data);
}
