#pragma once
#include <cstddef>
#include <string>
#include <vector>
class vtkDataObject;
namespace vortia {
struct FieldHistogramBin {double low{},high{},measure{};std::size_t cells{};};
struct FieldStatistics {
    double minimum{},maximum{},mean{},standardDeviation{},measure{},integral{};
    std::size_t cells{},excluded{};int dimension{};bool pointAverages{};
    std::vector<FieldHistogramBin> histogram;
};
FieldStatistics fieldStatistics(vtkDataObject*,const std::string& field,int bins=24);
}
