#pragma once

#include <array>
#include <cmath>

namespace vortia {

/// Minimal 3D vector of doubles for geometry, boundary values and domains.
struct Vec3 {
    double x{0.0};
    double y{0.0};
    double z{0.0};

    constexpr double operator[](std::size_t i) const {
        return i == 0 ? x : (i == 1 ? y : z);
    }
    constexpr double& operator[](std::size_t i) { return i == 0 ? x : (i == 1 ? y : z); }

    constexpr Vec3 operator+(const Vec3& o) const { return {x + o.x, y + o.y, z + o.z}; }
    constexpr Vec3 operator-(const Vec3& o) const { return {x - o.x, y - o.y, z - o.z}; }
    constexpr Vec3 operator*(double s) const { return {x * s, y * s, z * s}; }
    constexpr Vec3 operator/(double s) const { return {x / s, y / s, z / s}; }
    constexpr bool operator==(const Vec3&) const = default;

    [[nodiscard]] constexpr double dot(const Vec3& o) const {
        return x * o.x + y * o.y + z * o.z;
    }
    [[nodiscard]] constexpr Vec3 cross(const Vec3& o) const {
        return {y * o.z - z * o.y, z * o.x - x * o.z, x * o.y - y * o.x};
    }
    [[nodiscard]] double norm() const { return std::sqrt(dot(*this)); }
};

/// Axis-aligned bounding box.
struct Aabb {
    Vec3 min{};
    Vec3 max{};

    [[nodiscard]] constexpr Vec3 size() const { return max - min; }
    [[nodiscard]] constexpr Vec3 center() const { return (min + max) * 0.5; }
    [[nodiscard]] constexpr bool isValid() const {
        return min.x <= max.x && min.y <= max.y && min.z <= max.z;
    }
    void extend(const Vec3& p) {
        min.x = std::min(min.x, p.x);
        min.y = std::min(min.y, p.y);
        min.z = std::min(min.z, p.z);
        max.x = std::max(max.x, p.x);
        max.y = std::max(max.y, p.y);
        max.z = std::max(max.z, p.z);
    }
};

} // namespace vortia
