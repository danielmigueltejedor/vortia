#pragma once
#include "vortia/core/Vec3.hpp"
#include <array>
#include <string>
#include <vector>
class vtkDataObject;
namespace vortia {
struct LineSample { Vec3 position; double distance; double value; bool valid; };
std::array<double,6> fieldBounds(vtkDataObject* data);
std::vector<LineSample> sampleFieldLine(vtkDataObject* data,Vec3 start,Vec3 end,
                                      int points,const std::string& field);
}
