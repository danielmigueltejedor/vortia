#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <string>

namespace vortia {

/// Structured planar surrogate for an axisymmetric nozzle. Uses
/// fluid region between the centreline and wall, one cell in z.
struct PlanarNozzleMeshOptions {
    int convergingSegments{18};
    int divergingSegments{30};
    int axialCellsPerSegment{3};
    int radialCells{40};
    double radialGrading{2.0};      ///< Last/first cell-width ratio (toward wall).
    double thicknessMm{1.0};       ///< Exactly one cell across z.
};

/// Does NOT make a rotational wedge or a compressible CFD setup.
/// Generated blockMeshDict can be validated with OpenFOAM blockMesh/checkMesh.
class PlanarNozzleBlockMesh {
public:
    [[nodiscard]] static Expected<std::string>
    generate(const NozzleDimensions& geometry,
             const PlanarNozzleMeshOptions& mesh = {});
};

} // namespace vortia
