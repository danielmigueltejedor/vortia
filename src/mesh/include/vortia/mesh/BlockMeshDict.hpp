#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/geometry/BoxDomain.hpp"

#include <array>
#include <string>

namespace vortia {

/// OpenFOAM base patch categories usable in blockMeshDict `boundary` blocks.
enum class PatchKind { Wall, Patch, Empty, Symmetry };

[[nodiscard]] std::string_view toString(PatchKind kind);

/// Name + base type assigned to one face of the domain box.
struct FacePatch {
    BoxDomain::Face face;
    std::string name;
    PatchKind kind{PatchKind::Wall};
};

/// All parameters needed to emit a `system/blockMeshDict` for a box domain.
struct BlockMeshParams {
    std::array<int, 3> cells{20, 20, 1};          ///< Cells per direction (x, y, z).
    std::array<double, 3> grading{1.0, 1.0, 1.0}; ///< simpleGrading ratios per direction.
    double convertToMeters{1.0};                  ///< Unit scale of vertex coordinates.
    std::array<FacePatch, 6> patches{{
        {BoxDomain::XMin, "xmin", PatchKind::Wall},
        {BoxDomain::XMax, "xmax", PatchKind::Wall},
        {BoxDomain::YMin, "ymin", PatchKind::Wall},
        {BoxDomain::YMax, "ymax", PatchKind::Wall},
        {BoxDomain::ZMin, "zmin", PatchKind::Wall},
        {BoxDomain::ZMax, "zmax", PatchKind::Wall},
    }};
};

/// Generates the textual content of `system/blockMeshDict`.
/// Output is deterministic (golden-file testable) and compatible with both
/// OpenFOAM distributions for the features used here.
class BlockMeshDict {
public:
    [[nodiscard]] static Expected<std::string> generate(const BoxDomain& domain,
                                                        const BlockMeshParams& params);
};

} // namespace vortia
