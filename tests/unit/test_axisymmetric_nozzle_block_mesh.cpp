#include <gtest/gtest.h>
#include "vortia/mesh/AxisymmetricNozzleBlockMesh.hpp"

#include <string>
using namespace vortia;

TEST(AxisymmetricNozzleBlockMesh, EmitsCollapsedSectorWithWedgePatches) {
    AxisymmetricNozzleMeshOptions opts{4,6,2,20,3,5};
    auto mesh=AxisymmetricNozzleBlockMesh::generate({100,52,154,160,418},opts);
    ASSERT_TRUE(mesh) << mesh.error().describe();
    const std::string& s=mesh.value();
    EXPECT_NE(s.find("convertToMeters 0.001;"),std::string::npos);
    EXPECT_NE(s.find("hex (0 3 4 1 0 3 5 2) (2 20 1) simpleGrading (1 3 1)"),std::string::npos);
    EXPECT_NE(s.find("hex (27 30 31 28 27 30 32 29)"),std::string::npos);
    EXPECT_NE(s.find("type wedge;"),std::string::npos);
    EXPECT_NE(s.find("wedgeFront\n"),std::string::npos);
    EXPECT_NE(s.find("wedgeBack\n"),std::string::npos);
    EXPECT_NE(s.find("axis\n    {\n        type empty;"),std::string::npos);
    EXPECT_NE(s.find("        (0 3 3 0)"),std::string::npos);
    EXPECT_NE(s.find("    inlet\n    {\n        type patch;"),std::string::npos);
    EXPECT_NE(s.find("    nozzleWall\n    {\n        type wall;"),std::string::npos);
    EXPECT_NE(s.find("// Mesh only"),std::string::npos);
}
TEST(AxisymmetricNozzleBlockMesh, InvalidOptionsRejected) {
    AxisymmetricNozzleMeshOptions opts{};
    opts.wedgeAngleDegrees=8;
    EXPECT_FALSE(AxisymmetricNozzleBlockMesh::generate({},opts));
    opts={};
    opts.radialCells=0;
    EXPECT_FALSE(AxisymmetricNozzleBlockMesh::generate({},opts));
    opts={};
    opts.radialGrading=0;
    EXPECT_FALSE(AxisymmetricNozzleBlockMesh::generate({},opts));
    opts={};
    opts.convergingSegments=200;
    opts.divergingSegments=200;
    opts.radialCells=100;
    opts.axialCellsPerSegment=100;
    EXPECT_FALSE(AxisymmetricNozzleBlockMesh::generate({},opts));
}
TEST(AxisymmetricNozzleBlockMesh, StableOutput) {
    const auto a=AxisymmetricNozzleBlockMesh::generate({});
    const auto b=AxisymmetricNozzleBlockMesh::generate({});
    ASSERT_TRUE(a && b);
    EXPECT_EQ(a.value(),b.value());
}
