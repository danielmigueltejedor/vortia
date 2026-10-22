#include "vortia/geometry/TriangleMesh.hpp"

#include <algorithm>
#include <cmath>
#include <unordered_map>

namespace vortia {

Expected<void> TriangleMesh::validate() const {
    const auto n = static_cast<std::uint32_t>(vertices_.size());
    for (std::size_t i = 0; i < triangles_.size(); ++i) {
        for (const auto idx : triangles_[i]) {
            if (idx >= n)
                return Error::make(ErrorCode::InvalidData,
                                   "triangle " + std::to_string(i) +
                                       " references vertex out of range");
        }
    }
    if (vertices_.empty() && !triangles_.empty())
        return Error::make(ErrorCode::InvalidData, "mesh has triangles but no vertices");
    return {};
}

std::size_t TriangleMesh::countDegenerateTriangles(double areaEpsilon) const {
    std::size_t count = 0;
    for (const auto& t : triangles_) {
        const Vec3& a = vertices_[t[0]];
        const Vec3& b = vertices_[t[1]];
        const Vec3& c = vertices_[t[2]];
        const double twiceArea = (b - a).cross(c - a).norm();
        if (twiceArea <= 2.0 * areaEpsilon)
            ++count;
    }
    return count;
}

Aabb TriangleMesh::boundingBox() const {
    Aabb box{};
    if (vertices_.empty())
        return box;
    box.min = box.max = vertices_.front();
    for (const auto& v : vertices_)
        box.extend(v);
    return box;
}

namespace {

struct VertexKey {
    std::int64_t x, y, z;
    bool operator==(const VertexKey&) const = default;
};

struct VertexKeyHash {
    std::size_t operator()(const VertexKey& k) const {
        std::size_t h = static_cast<std::size_t>(k.x) * 0x9E3779B97F4A7C15ull;
        h ^= static_cast<std::size_t>(k.y) + 0x9E3779B9 + (h << 6) + (h >> 2);
        h ^= static_cast<std::size_t>(k.z) + 0x9E3779B9 + (h << 6) + (h >> 2);
        return h;
    }
};

} // namespace

std::size_t TriangleMesh::weldVertices(double tolerance) {
    if (vertices_.empty() || tolerance <= 0.0)
        return 0;

    const double inv = 1.0 / tolerance;
    std::unordered_map<VertexKey, std::uint32_t, VertexKeyHash> map;
    map.reserve(vertices_.size());

    std::vector<Vec3> unique;
    unique.reserve(vertices_.size());
    std::vector<std::uint32_t> remap(vertices_.size());

    for (std::size_t i = 0; i < vertices_.size(); ++i) {
        const Vec3& v = vertices_[i];
        const VertexKey key{static_cast<std::int64_t>(std::llround(v.x * inv)),
                            static_cast<std::int64_t>(std::llround(v.y * inv)),
                            static_cast<std::int64_t>(std::llround(v.z * inv))};
        auto [it, inserted] = map.emplace(key, static_cast<std::uint32_t>(unique.size()));
        if (inserted)
            unique.push_back(v);
        remap[i] = it->second;
    }

    const std::size_t removed = vertices_.size() - unique.size();
    vertices_ = std::move(unique);
    for (auto& t : triangles_)
        for (auto& idx : t)
            idx = remap[idx];
    return removed;
}

} // namespace vortia
