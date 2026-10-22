#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Vec3.hpp"

#include <array>
#include <vector>

namespace vortia {

/// Rectangular (axis-aligned) computational domain — the MVP geometry.
/// Vertices follow the OpenFOAM `hex` cell ordering:
///   0:(min)  1:(x max)  2:(x,y max)  3:(y max)        (z = min plane)
///   4..7: same pattern on the z = max plane.
class BoxDomain {
public:
    /// Default: invalid domain (min == max). Use fromExtents for validation.
    BoxDomain() = default;

    [[nodiscard]] static Expected<BoxDomain> fromExtents(const Vec3& min, const Vec3& max);

    [[nodiscard]] bool isValid() const {
        return max_.x > min_.x && max_.y > min_.y && max_.z > min_.z;
    }

    [[nodiscard]] const Vec3& min() const { return min_; }
    [[nodiscard]] const Vec3& max() const { return max_; }
    [[nodiscard]] Vec3 size() const { return max_ - min_; }
    [[nodiscard]] Vec3 center() const { return (min_ + max_) * 0.5; }

    /// The 8 corner vertices in OpenFOAM hex ordering.
    [[nodiscard]] std::array<Vec3, 8> blockVertices() const;

    /// The 6 outward-facing boundary quads (vertex indices into blockVertices),
    /// one per axis direction: -x, +x, -y, +y, -z, +z.
    enum Face : int { XMin = 0, XMax, YMin, YMax, ZMin, ZMax };
    [[nodiscard]] static std::array<std::array<int, 4>, 6> faceVertices();

private:
    Vec3 min_{};
    Vec3 max_{};
};

} // namespace vortia
