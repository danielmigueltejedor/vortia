#include "vortia/geometry/SketchTemplates.hpp"
#include <gtest/gtest.h>
#include <numbers>
#include <limits>

using namespace vortia;

namespace {
void expectTangent(const Sketch2D& s, std::size_t segment) {
    const auto c = s.arc(segment);
    ASSERT_TRUE(c);
    const auto n = s.points().size();
    const auto p = s.points()[(segment + n - 1) % n];
    const auto a = s.points()[segment];
    const auto b = s.points()[(segment + 1) % n];
    const auto q = s.points()[(segment + 2) % n];
    EXPECT_NEAR((a.xMm-p.xMm)*(a.xMm-c->center.xMm)
                + (a.yMm-p.yMm)*(a.yMm-c->center.yMm), 0, 1e-8);
    EXPECT_NEAR((q.xMm-b.xMm)*(b.xMm-c->center.xMm)
                + (q.yMm-b.yMm)*(b.yMm-c->center.yMm), 0, 1e-8);
}
}

TEST(SketchArcs, ExactQuarterCircleTangencyLengthAndArea) {
    auto s = makeRectangleSketch(100, 50).value();
    ASSERT_TRUE(s.filletVertex(1, 10));
    ASSERT_EQ(s.points().size(), 5U);
    EXPECT_NEAR(s.points()[1].xMm, 90, 1e-12);
    EXPECT_NEAR(s.points()[2].yMm, 10, 1e-12);
    const auto arc = s.arc(1).value();
    EXPECT_NEAR(arc.radiusMm, 10, 1e-12);
    EXPECT_NEAR(arc.center.xMm, 90, 1e-12);
    EXPECT_NEAR(arc.center.yMm, 10, 1e-12);
    EXPECT_NEAR(s.segmentLength(1), 5*std::numbers::pi, 1e-12);
    EXPECT_NEAR(s.perimeter(), 280 + 5*std::numbers::pi, 1e-10);
    EXPECT_NEAR(s.signedArea(), 4900 + 25*std::numbers::pi, 1e-9);
    expectTangent(s, 1);
}

TEST(SketchArcs, ClockwiseOpenCornerAndEditableRadius) {
    Sketch2D s;
    ASSERT_TRUE(s.addPoint({0,0}));
    ASSERT_TRUE(s.addPoint({0,100}));
    ASSERT_TRUE(s.addPoint({100,100}));
    ASSERT_TRUE(s.filletVertex(1, 20));
    EXPECT_LT(s.arc(1)->sweepRadians, 0);
    expectTangent(s, 1);
    ASSERT_TRUE(s.setFilletRadius(1, 30));
    EXPECT_NEAR(s.arc(1)->radiusMm, 30, 1e-10);
    EXPECT_NEAR(s.points()[1].yMm, 70, 1e-10);
    EXPECT_NEAR(s.points()[2].xMm, 30, 1e-10);
    EXPECT_DOUBLE_EQ(s.points()[0].yMm, 0);
    EXPECT_DOUBLE_EQ(s.points()[3].xMm, 100);
    expectTangent(s, 1);
}

TEST(SketchArcs, SplitRetainsCircleLengthAndAreaIncludingClosingArc) {
    auto s = makeRectangleSketch(100, 50).value();
    ASSERT_TRUE(s.filletVertex(0, 10));
    // Put an independent arc on the closing edge to exercise wraparound.
    ASSERT_TRUE(s.setSegmentBulge(s.segmentCount()-1, -0.25));
    const auto index = s.segmentCount()-1;
    const auto midpoint = s.segmentPoint(index, 0.3);
    const auto before = s.arc(index).value();
    const double length = s.perimeter(), area = s.signedArea();
    ASSERT_TRUE(s.splitSegment(index, 0.3));
    EXPECT_NEAR(s.perimeter(), length, 1e-9);
    EXPECT_NEAR(s.signedArea(), area, 1e-9);
    EXPECT_NEAR(s.points().back().xMm, midpoint.xMm, 1e-12);
    EXPECT_NEAR(s.points().back().yMm, midpoint.yMm, 1e-12);
    EXPECT_NEAR(s.arc(index)->radiusMm, before.radiusMm, 1e-9);
    EXPECT_NEAR(s.arc(index+1)->center.xMm, before.center.xMm, 1e-9);
    EXPECT_NEAR(s.arc(index+1)->center.yMm, before.center.yMm, 1e-9);
}

TEST(SketchArcs, TwoNozzleCornersRemainTangentWhenResized) {
    Sketch2D s;
    for (const auto p : {SketchPoint2D{0,100},{100,100},{172,52},{750,154}})
        ASSERT_TRUE(s.addPoint(p));
    ASSERT_TRUE(s.filletVertex(1, 60));
    ASSERT_TRUE(s.filletVertex(3, 20));
    expectTangent(s, 1);
    expectTangent(s, 3);
    ASSERT_TRUE(s.setFilletRadius(1, 50));
    ASSERT_TRUE(s.setFilletRadius(3, 25));
    expectTangent(s, 1);
    expectTangent(s, 3);
}

TEST(SketchArcs, InvalidEditsLeaveGeometryUnchanged) {
    auto s = makeRectangleSketch(100, 50).value();
    ASSERT_TRUE(s.filletVertex(1, 10));
    const auto original = s;
    EXPECT_FALSE(s.setFilletRadius(1, 100));
    EXPECT_FALSE(s.setFilletRadius(1, std::numeric_limits<double>::quiet_NaN()));
    EXPECT_FALSE(s.filletVertex(2, 5));
    EXPECT_FALSE(s.movePoint(1, {80,0}));
    EXPECT_FALSE(s.movePoint(0, {0,5}));
    EXPECT_FALSE(s.setSegmentLength(1, 1));
    EXPECT_FALSE(s.setSegmentLength(0, 10));
    EXPECT_FALSE(s.setSegmentBulge(1, 2));
    EXPECT_FALSE(s.setSegmentBulge(1, 1e-300));
    EXPECT_FALSE(s.splitSegment(1, 1e-15));
    EXPECT_DOUBLE_EQ(s.perimeter(), original.perimeter());
    EXPECT_DOUBLE_EQ(s.signedArea(), original.signedArea());
    EXPECT_DOUBLE_EQ(s.segmentBulge(1), original.segmentBulge(1));
    for (std::size_t i = 0; i < s.points().size(); ++i) {
        EXPECT_DOUBLE_EQ(s.points()[i].xMm, original.points()[i].xMm);
        EXPECT_DOUBLE_EQ(s.points()[i].yMm, original.points()[i].yMm);
    }
}

TEST(SketchArcs, RejectsStraightUTurnEndpointsAndOversizedFillets) {
    Sketch2D s;
    for (const auto p : {SketchPoint2D{0,0},{10,0},{20,0}}) ASSERT_TRUE(s.addPoint(p));
    EXPECT_FALSE(s.filletVertex(0, 1));
    EXPECT_FALSE(s.filletVertex(2, 1));
    EXPECT_FALSE(s.filletVertex(1, 1));
    ASSERT_TRUE(s.movePoint(2, {0,0}));
    EXPECT_FALSE(s.filletVertex(1, 1));
    auto r = makeRectangleSketch(10,10).value();
    EXPECT_FALSE(r.filletVertex(0, 10));
    EXPECT_FALSE(r.filletVertex(0, 0));
    EXPECT_FALSE(r.filletVertex(0, -1));
    ASSERT_TRUE(r.filletVertex(0, 1));
    expectTangent(r, 0);
}
