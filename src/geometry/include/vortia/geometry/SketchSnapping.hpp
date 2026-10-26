#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <vector>
namespace vortia {
struct SketchSnapOptions {
    bool enabled=true,endpoints=true,midpoints=true,centers=true,alignment=true,perpendicular=true;
    bool orthogonal=false,polar=false;
    double polarDegrees=15;
};
enum class SketchSnapKind { None, Endpoint, Midpoint, Center, Alignment, Perpendicular, Orthogonal, Polar };
struct SketchSnapResult {
    SketchPoint2D point;
    SketchSnapKind kind=SketchSnapKind::None;
    std::vector<SketchPoint2D> references;
};
/// A placement aid, not a persistent geometric constraint. tolerance is in mm.
SketchSnapResult snapSketchPoint(const Sketch2D&,SketchPoint2D cursor,double tolerance,const SketchSnapOptions&);
}
