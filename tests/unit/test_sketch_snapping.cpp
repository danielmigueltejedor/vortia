#include "vortia/geometry/SketchSnapping.hpp"
#include <gtest/gtest.h>
#include <numbers>
using namespace vortia;
TEST(SketchSnapping, ExactEndpointsTakePriorityOverGridAndTracking){
    Sketch2D s;ASSERT_TRUE(s.addPoint({.123,.789}));ASSERT_TRUE(s.addPoint({10,10}));SketchSnapOptions options;
    const auto hit=snapSketchPoint(s,{.2,.8},.2,options);EXPECT_EQ(hit.kind,SketchSnapKind::Endpoint);EXPECT_DOUBLE_EQ(hit.point.xMm,.123);EXPECT_DOUBLE_EQ(hit.point.yMm,.789);
    options.enabled=false;const auto free=snapSketchPoint(s,{.2,.8},.2,options);EXPECT_EQ(free.kind,SketchSnapKind::None);EXPECT_DOUBLE_EQ(free.point.xMm,.2);
}
TEST(SketchSnapping, MidpointsAlignmentAndPerpendicularPlacementAreGeometric){
    Sketch2D s;ASSERT_TRUE(s.addPoint({0,0}));ASSERT_TRUE(s.addPoint({10,0}));SketchSnapOptions options;
    const auto mid=snapSketchPoint(s,{5,.05},.1,options);EXPECT_EQ(mid.kind,SketchSnapKind::Midpoint);EXPECT_DOUBLE_EQ(mid.point.xMm,5);EXPECT_DOUBLE_EQ(mid.point.yMm,0);
    const auto perpendicular=snapSketchPoint(s,{10.05,7},.1,options);EXPECT_EQ(perpendicular.kind,SketchSnapKind::Perpendicular);EXPECT_DOUBLE_EQ(perpendicular.point.xMm,10);EXPECT_DOUBLE_EQ(perpendicular.point.yMm,7);
    options.perpendicular=false;const auto aligned=snapSketchPoint(s,{.05,7},.1,options);EXPECT_EQ(aligned.kind,SketchSnapKind::Alignment);EXPECT_DOUBLE_EQ(aligned.point.xMm,0);
    options.orthogonal=true;const auto ortho=snapSketchPoint(s,{17,3},.1,options);EXPECT_EQ(ortho.kind,SketchSnapKind::Orthogonal);EXPECT_DOUBLE_EQ(ortho.point.xMm,17);EXPECT_DOUBLE_EQ(ortho.point.yMm,0);
}
TEST(SketchSnapping, PolarAndArcCentersUseExactGeometry){
    Sketch2D s;ASSERT_TRUE(s.addPoint({0,0}));SketchSnapOptions options;options.alignment=false;options.polar=true;
    const double angle=30*std::numbers::pi/180;const auto polar=snapSketchPoint(s,{10*std::cos(angle)+.01,10*std::sin(angle)},.1,options);
    EXPECT_EQ(polar.kind,SketchSnapKind::Polar);EXPECT_NEAR(polar.point.yMm/polar.point.xMm,std::tan(angle),1e-12);
    ASSERT_TRUE(s.addPoint({10,0}));ASSERT_TRUE(s.setSegmentBulge(0,1));const auto center=snapSketchPoint(s,{5,.01},.1,options);EXPECT_EQ(center.kind,SketchSnapKind::Center);EXPECT_NEAR(center.point.xMm,5,1e-12);EXPECT_NEAR(center.point.yMm,0,1e-12);
}
TEST(SketchDeletion, PreservesUnrelatedArcsAndOpensProfilesBelowThreeVertices){
    Sketch2D s;for(const auto p:{SketchPoint2D{0,0},{10,0},{10,10},{0,10},{-5,5}})ASSERT_TRUE(s.addPoint(p));ASSERT_TRUE(s.close());ASSERT_TRUE(s.setSegmentBulge(2,.2));
    ASSERT_TRUE(s.removePoint(0));EXPECT_TRUE(s.closed());EXPECT_EQ(s.points().size(),4);EXPECT_DOUBLE_EQ(s.segmentBulge(1),.2);EXPECT_DOUBLE_EQ(s.segmentBulge(3),0);
    ASSERT_TRUE(s.removePoint(0));ASSERT_TRUE(s.removePoint(0));EXPECT_FALSE(s.closed());EXPECT_EQ(s.points().size(),2);EXPECT_FALSE(s.removePoint(9));
}
