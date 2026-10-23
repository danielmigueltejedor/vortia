#include "vortia/geometry/SketchTemplates.hpp"
#include <gtest/gtest.h>
#include <algorithm>
#include <cmath>
using namespace vortia;

TEST(SketchTemplates, ExactRectangleMeasures) {
    auto rect=makeRectangleSketch(154,52);
    ASSERT_TRUE(rect.has_value());
    EXPECT_EQ(rect->points().size(),4u);
    EXPECT_TRUE(rect->closed());
    EXPECT_DOUBLE_EQ(rect->signedArea(),154*52);
    EXPECT_DOUBLE_EQ(rect->segmentLength(0),154);
}
TEST(SketchTemplates, CircularProfileApproximatesRequestedRadius) {
    auto circle=makeCircleSketch(100,96);
    ASSERT_TRUE(circle.has_value());
    EXPECT_TRUE(circle->closed());
    EXPECT_NEAR(circle->signedArea(), std::acos(-1.0)*2500,20.0);
}
TEST(SketchTemplates, NozzleMaintainsInletThroatOutletDiameters) {
    const NozzleDimensions params{100,52,154,160,418};
    auto nozzle=makeNozzleProfile(params);
    ASSERT_TRUE(nozzle);
    const auto& pts=nozzle->points();
    EXPECT_NEAR(pts[1].xMm,578,1e-9);
    EXPECT_NEAR(pts[2].yMm,77,1e-9);
    bool foundThroat=false, foundInlet=false;
    for(const auto& p:pts) {
        if(std::abs(p.xMm-160)<1e-9 && std::abs(p.yMm-26)<1e-9)foundThroat=true;
        if(std::abs(p.xMm)<1e-9 && std::abs(p.yMm-50)<1e-9)foundInlet=true;
    }
    EXPECT_TRUE(foundThroat);
    EXPECT_TRUE(foundInlet);
    EXPECT_GT(nozzle->signedArea(),0);
}
TEST(SketchTemplates, RejectNonphysicalDimensions) {
    EXPECT_FALSE(makeNozzleProfile(NozzleDimensions{20,30,50,100,100}));
    EXPECT_FALSE(makeRectangleSketch(-1,5));
    EXPECT_FALSE(makeCircleSketch(0));
    EXPECT_FALSE(makeNozzleProfile(NozzleDimensions{100,NAN,150,160,418}));
}
