#include <gtest/gtest.h>

#include "vortia/geometry/BoxDomain.hpp"

using namespace vortia;

TEST(BoxDomain, RejectsInvertedExtents) {
    EXPECT_FALSE(BoxDomain::fromExtents({1, 0, 0}, {0, 1, 1}).hasValue());
    EXPECT_FALSE(BoxDomain::fromExtents({0, 0, 0}, {0, 1, 1}).hasValue()); // degenerate x
    EXPECT_FALSE(BoxDomain::fromExtents({0, 0, 0}, {1, NAN, 1}).hasValue());
}

TEST(BoxDomain, BlockVerticesFollowOpenFoamHexOrdering) {
    auto box = BoxDomain::fromExtents({0, 0, 0}, {2, 1, 0.5});
    ASSERT_TRUE(box.hasValue());
    const auto v = box->blockVertices();
    EXPECT_EQ(v[0], (Vec3{0, 0, 0}));
    EXPECT_EQ(v[1], (Vec3{2, 0, 0}));
    EXPECT_EQ(v[2], (Vec3{2, 1, 0}));
    EXPECT_EQ(v[3], (Vec3{0, 1, 0}));
    EXPECT_EQ(v[4], (Vec3{0, 0, 0.5}));
    EXPECT_EQ(v[7], (Vec3{0, 1, 0.5}));
}

TEST(BoxDomain, BoundaryFacesPointOutwards) {
    auto box = BoxDomain::fromExtents({-1, -2, -3}, {4, 5, 6});
    ASSERT_TRUE(box.hasValue());
    const auto vertices = box->blockVertices();
    const auto faces = BoxDomain::faceVertices();
    const Vec3 center = box->center();

    // For each face, the normal via right-hand rule must point away from center.
    const char* names[6] = {"-x", "+x", "-y", "+y", "-z", "+z"};
    for (int f = 0; f < 6; ++f) {
        const auto& q = faces[f];
        const Vec3& a = vertices[q[0]];
        const Vec3& b = vertices[q[1]];
        const Vec3& c = vertices[q[2]];
        const Vec3 normal = (b - a).cross(c - b);
        const Vec3 faceCenter = (a + b + c + vertices[q[3]]) * 0.25;
        const Vec3 outward = faceCenter - center;
        EXPECT_GT(normal.dot(outward), 0.0) << "face " << names[f] << " normal points inwards";
    }
}
