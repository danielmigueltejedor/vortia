#include "vortia/geometry/BoxDomain.hpp"

#include <cmath>

namespace vortia {

Expected<BoxDomain> BoxDomain::fromExtents(const Vec3& min, const Vec3& max) {
    for (int axis = 0; axis < 3; ++axis) {
        if (!std::isfinite(min[axis]) || !std::isfinite(max[axis]))
            return Error::make(ErrorCode::InvalidArgument,
                               "domain extents must be finite numbers");
        if (!(max[axis] > min[axis]))
            return Error::make(ErrorCode::InvalidArgument,
                               "domain max must be greater than min on every axis");
    }
    BoxDomain box;
    box.min_ = min;
    box.max_ = max;
    return box;
}

std::array<Vec3, 8> BoxDomain::blockVertices() const {
    const auto& a = min_;
    const auto& b = max_;
    return {Vec3{a.x, a.y, a.z}, Vec3{b.x, a.y, a.z}, Vec3{b.x, b.y, a.z}, Vec3{a.x, b.y, a.z},
            Vec3{a.x, a.y, b.z}, Vec3{b.x, a.y, b.z}, Vec3{b.x, b.y, b.z}, Vec3{a.x, b.y, b.z}};
}

std::array<std::array<int, 4>, 6> BoxDomain::faceVertices() {
    // Outward normals verified against the right-hand rule (see unit tests).
    return {{{0, 4, 7, 3},  // -x
             {1, 2, 6, 5},  // +x
             {0, 1, 5, 4},  // -y
             {3, 7, 6, 2},  // +y
             {0, 3, 2, 1},  // -z
             {4, 5, 6, 7}}};// +z
}

} // namespace vortia
