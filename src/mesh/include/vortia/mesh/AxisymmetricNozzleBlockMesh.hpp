#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <string>

namespace vortia {

struct AxisymmetricNozzleMeshOptions {
    int convergingSegments{18};
    int divergingSegments{30};
    int axialCellsPerSegment{3};
    int radialCells{40};
    double radialGrading{2.0};
    double wedgeAngleDegrees{5.0};
};

/// Generates a collapsed-axis OpenFOAM wedge mesh as a blockMeshDict.
/// This is a mesh dictionary ONLY; fields, thermodynamics and solver-specific
/// boundary conditions still require a compressible case generator.
class AxisymmetricNozzleBlockMesh {
public:
    [[nodiscard]] static Expected<std::string>
    generate(const NozzleDimensions& dims,
             const AxisymmetricNozzleMeshOptions& options = {});
};

} // namespace vortia
