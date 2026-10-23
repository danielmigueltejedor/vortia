#include <gtest/gtest.h>
#include "vortia/mesh/PlanarNozzleBlockMesh.hpp"

#include <string>
using namespace vortia;

TEST(PlanarNozzleBlockMesh, EmitsConformingHexBlocksAndPhysicalPatches) {
    const NozzleDimensions dims{100,52,154,160,418};
    const PlanarNozzleMeshOptions opts{4,6,2,20,3,1};
    auto dict = PlanarNozzleBlockMesh::generate(dims,opts);
    ASSERT_TRUE(dict) << dict.error().describe();
    const auto& content = dict.value();
    EXPECT_NE(content.find("convertToMeters 0.001;"),std::string::npos);
    EXPECT_NE(content.find("    hex (0 4 5 1 2 6 7 3) (2 20 1) simpleGrading (1 3 1)"),std::string::npos);
    EXPECT_NE(content.find("hex (36 40 41 37 38 42 43 39)"),std::string::npos);
    EXPECT_NE(content.find("    inlet\n    {\n        type patch;"),std::string::npos);
    EXPECT_NE(content.find("    outlet\n    {\n        type patch;"),std::string::npos);
    EXPECT_NE(content.find("    axis\n    {\n        type symmetryPlane;"),std::string::npos);
    EXPECT_NE(content.find("    nozzleWall\n    {\n        type wall;"),std::string::npos);
    EXPECT_NE(content.find("    frontAndBack\n    {\n        type empty;"),std::string::npos);
    // Throat at x = 160 mm, r = 26 mm, outlet x = 578 mm.
    EXPECT_NE(content.find("(160 26 0)"),std::string::npos);
    EXPECT_NE(content.find("(578 77 0)"),std::string::npos);
}
TEST(PlanarNozzleBlockMesh, Deterministic) {
    const auto first = PlanarNozzleBlockMesh::generate({});
    const auto second = PlanarNozzleBlockMesh::generate({});
    ASSERT_TRUE(first && second);
    EXPECT_EQ(first.value(),second.value());
}
TEST(PlanarNozzleBlockMesh, RejectsBadSizesAndExcessiveCells) {
    auto opts = PlanarNozzleMeshOptions{};
    opts.radialCells = 0;
    EXPECT_FALSE(PlanarNozzleBlockMesh::generate({},opts));
    opts = {};
    opts.radialGrading = -2;
    EXPECT_FALSE(PlanarNozzleBlockMesh::generate({},opts));
    opts = {};
    opts.convergingSegments = 200;
    opts.divergingSegments = 200;
    opts.axialCellsPerSegment = 100;
    opts.radialCells = 100;
    EXPECT_FALSE(PlanarNozzleBlockMesh::generate({},opts));
    EXPECT_FALSE(PlanarNozzleBlockMesh::generate({100,120,160,180,400}));
}
