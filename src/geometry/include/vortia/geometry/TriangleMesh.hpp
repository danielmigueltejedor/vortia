#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Vec3.hpp"

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace vortia {

/// Indexed triangle soup: the exchange format for imported STL surfaces.
/// This is NOT a solid CAD model — it carries no topology or parametric
/// information (see docs/architecture/research.md §7).
class TriangleMesh {
public:
    using Triangle = std::array<std::uint32_t, 3>;

    void addVertex(const Vec3& v) { vertices_.push_back(v); }
    void addTriangle(const Triangle& t) { triangles_.push_back(t); }
    void reserve(std::size_t vertexCount, std::size_t triangleCount) {
        vertices_.reserve(vertexCount);
        triangles_.reserve(triangleCount);
    }

    [[nodiscard]] const std::vector<Vec3>& vertices() const { return vertices_; }
    [[nodiscard]] const std::vector<Triangle>& triangles() const { return triangles_; }
    [[nodiscard]] bool isEmpty() const { return triangles_.empty(); }

    /// Checks index bounds and counts degenerate (zero-area) triangles.
    [[nodiscard]] Expected<void> validate() const;
    [[nodiscard]] std::size_t countDegenerateTriangles(double areaEpsilon = 1e-18) const;
    [[nodiscard]] Aabb boundingBox() const;

    /// Merges coincident vertices (STL stores triangles independently).
    [[nodiscard]] std::size_t weldVertices(double tolerance = 1e-9);

    [[nodiscard]] const std::string& sourceDescription() const { return source_; }
    void setSourceDescription(std::string s) { source_ = std::move(s); }

private:
    std::vector<Vec3> vertices_;
    std::vector<Triangle> triangles_;
    std::string source_;
};

} // namespace vortia
