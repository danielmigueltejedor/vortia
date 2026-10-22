#include <gtest/gtest.h>

#include "vortia/geometry/TriangleMesh.hpp"

using namespace vortia;

TEST(TriangleMesh, ValidateDetectsOutOfRangeIndex) {
    TriangleMesh mesh;
    mesh.addVertex({0, 0, 0});
    mesh.addTriangle({0, 1, 2});
    EXPECT_FALSE(mesh.validate().hasValue());
}

TEST(TriangleMesh, BoundingBox) {
    TriangleMesh mesh;
    mesh.addVertex({-1, 2, 0});
    mesh.addVertex({3, -4, 1});
    mesh.addVertex({0, 0, 5});
    const Aabb box = mesh.boundingBox();
    EXPECT_DOUBLE_EQ(box.min.x, -1);
    EXPECT_DOUBLE_EQ(box.min.y, -4);
    EXPECT_DOUBLE_EQ(box.min.z, 0);
    EXPECT_DOUBLE_EQ(box.max.x, 3);
    EXPECT_DOUBLE_EQ(box.max.y, 2);
    EXPECT_DOUBLE_EQ(box.max.z, 5);
}

TEST(TriangleMesh, DegenerateCount) {
    TriangleMesh mesh;
    mesh.addVertex({0, 0, 0});
    mesh.addVertex({1, 0, 0});
    mesh.addVertex({2, 0, 0}); // collinear → zero area
    mesh.addVertex({0, 1, 0});
    mesh.addTriangle({0, 1, 2}); // degenerate
    mesh.addTriangle({0, 1, 3}); // fine
    EXPECT_EQ(mesh.countDegenerateTriangles(), 1u);
}

TEST(TriangleMesh, WeldMergesCoincidentVertices) {
    TriangleMesh mesh;
    // Two triangles sharing an edge, stored with duplicated vertices (STL style).
    mesh.addVertex({0, 0, 0});
    mesh.addVertex({1, 0, 0});
    mesh.addVertex({0, 1, 0});
    mesh.addVertex({1, 0, 0});
    mesh.addVertex({1, 1, 0});
    mesh.addVertex({0, 1, 0});
    mesh.addTriangle({0, 1, 2});
    mesh.addTriangle({3, 4, 5});
    const std::size_t removed = mesh.weldVertices();
    EXPECT_EQ(removed, 2u); // (1,0,0) and (0,1,0) were duplicated
    EXPECT_EQ(mesh.vertices().size(), 4u);
    EXPECT_TRUE(mesh.validate().hasValue());
    // Connectivity preserved: second triangle now references shared vertices.
    EXPECT_EQ(mesh.triangles()[1][0], mesh.triangles()[0][1]);
    EXPECT_EQ(mesh.triangles()[1][2], mesh.triangles()[0][2]);
}
