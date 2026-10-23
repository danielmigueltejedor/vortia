#include <gtest/gtest.h>
#include "vortia/geometry/Sketch2D.hpp"
#include <cmath>

using vortia::Sketch2D;
using vortia::SketchPoint2D;

TEST(Sketch2D, ExactDimensionsInMillimetres) {
    Sketch2D s;
    ASSERT_TRUE(s.addPoint({0, 0}));
    ASSERT_TRUE(s.addPoint({100, 0}));
    EXPECT_DOUBLE_EQ(s.segmentLength(0), 100);
    ASSERT_TRUE(s.setSegmentLength(0, 154));
    EXPECT_NEAR(s.points()[1].xMm, 154, 1e-12);
    EXPECT_DOUBLE_EQ(s.segmentLength(0), 154);
}
TEST(Sketch2D, SplitsAnExistingSegment) {
    Sketch2D s;
    ASSERT_TRUE(s.addPoint({0,0}));
    ASSERT_TRUE(s.addPoint({100,0}));
    ASSERT_TRUE(s.splitSegment(0));
    ASSERT_EQ(s.segmentCount(), 2u);
    EXPECT_NEAR(s.points()[1].xMm, 50, 1e-12);
    EXPECT_DOUBLE_EQ(s.perimeter(), 100);
}
TEST(Sketch2D, ClosedGeometryAndArea) {
    Sketch2D s;
    ASSERT_TRUE(s.addPoint({0,0}));
    ASSERT_TRUE(s.addPoint({100,0}));
    ASSERT_TRUE(s.addPoint({100,50}));
    ASSERT_TRUE(s.addPoint({0,50}));
    ASSERT_TRUE(s.close());
    EXPECT_DOUBLE_EQ(s.signedArea(), 5000);
    EXPECT_DOUBLE_EQ(s.perimeter(), 300);
    ASSERT_TRUE(s.splitSegment(3));
    EXPECT_EQ(s.points().size(),5u);
    EXPECT_DOUBLE_EQ(s.signedArea(), 5000);
}
TEST(Sketch2D, ProtectsAgainstInvalidDegenerateGeometry) {
    Sketch2D s;
    ASSERT_TRUE(s.addPoint({0,0}));
    EXPECT_FALSE(s.addPoint({0,0}));
    EXPECT_FALSE(s.addPoint({NAN,5}));
    ASSERT_TRUE(s.addPoint({10,0}));
    EXPECT_FALSE(s.close());
    EXPECT_FALSE(s.setSegmentLength(0,-2));
    EXPECT_FALSE(s.splitSegment(0,1.2));
}
