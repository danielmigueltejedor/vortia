#pragma once
#include "vortia/core/Vec3.hpp"
#include <array>
#include <string>
#include <vector>
#include <vtkSmartPointer.h>
class vtkDataObject;
class vtkPolyData;
namespace vortia {
struct LineSample { Vec3 position; double distance; double value; bool valid; };
std::array<double,6> fieldBounds(vtkDataObject* data);
std::vector<std::string> fieldNames(vtkDataObject* data);
LineSample sampleFieldPoint(vtkDataObject* data,Vec3 point,const std::string& field);
vtkSmartPointer<vtkPolyData> sliceField(vtkDataObject* data,int axis,double position);
std::vector<LineSample> sampleFieldLine(vtkDataObject* data,Vec3 start,Vec3 end,
                                      int points,const std::string& field);
}
