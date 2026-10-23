#include <gtest/gtest.h>

#include "vortia/mesh/BlockMeshDict.hpp"

using namespace vortia;

namespace {

BoxDomain unitBox() { return BoxDomain::fromExtents({0, 0, 0}, {1, 1, 1}).value(); }

/// The classic lid-driven cavity mesh: 2D (1 cell in z), empty front/back.
BlockMeshParams cavityParams() {
    BlockMeshParams p;
    p.cells = {20, 20, 1};
    p.patches = {{
        {BoxDomain::XMin, "fixedWalls", PatchKind::Wall},
        {BoxDomain::XMax, "fixedWalls2", PatchKind::Wall},
        {BoxDomain::YMin, "movingWall", PatchKind::Wall},
        {BoxDomain::YMax, "fixedWalls3", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    return p;
}

} // namespace

TEST(BlockMeshDict, ContainsAllStructuralBlocks) {
    auto dict = BlockMeshDict::generate(unitBox(), BlockMeshParams{});
    ASSERT_TRUE(dict.hasValue()) << dict.error().describe();
    const std::string& s = dict.value();
    EXPECT_NE(s.find("FoamFile"), std::string::npos);
    EXPECT_NE(s.find("object      blockMeshDict;"), std::string::npos);
    EXPECT_NE(s.find("convertToMeters 1;"), std::string::npos);
    EXPECT_NE(s.find("vertices\n("), std::string::npos);
    EXPECT_NE(s.find("hex (0 1 2 3 4 5 6 7) (20 20 1) simpleGrading (1 1 1)"),
              std::string::npos);
    EXPECT_NE(s.find("boundary\n("), std::string::npos);
}

TEST(BlockMeshDict, EmitsPatchTypes) {
    auto dict = BlockMeshDict::generate(unitBox(), cavityParams());
    ASSERT_TRUE(dict.hasValue());
    const std::string& s = dict.value();
    EXPECT_NE(s.find("movingWall\n    {\n        type wall;"), std::string::npos);
    EXPECT_NE(s.find("frontAndBack\n    {\n        type empty;"), std::string::npos);
}

TEST(BlockMeshDict, EmitsAllEightVertices) {
    auto dict = BlockMeshDict::generate(unitBox(), BlockMeshParams{});
    ASSERT_TRUE(dict.hasValue());
    // Vertex block: between "vertices" and ");" there must be 8 lines.
    const auto begin = dict->find("vertices\n(\n") + std::string("vertices\n(\n").size();
    const auto end = dict->find(");", begin);
    int lines = 0;
    for (auto pos = begin; pos < end; pos = dict->find('\n', pos) + 1)
        ++lines;
    EXPECT_EQ(lines, 8);
}

TEST(BlockMeshDict, RejectsInvalidCells) {
    BlockMeshParams p;
    p.cells = {0, 20, 1};
    EXPECT_FALSE(BlockMeshDict::generate(unitBox(), p).hasValue());
}

TEST(BlockMeshDict, RejectsDuplicatePatchNames) {
    BlockMeshParams p;
    for (auto& patch : p.patches)
        patch.name = "same";
    EXPECT_FALSE(BlockMeshDict::generate(unitBox(), p).hasValue());
}

TEST(BlockMeshDict, RejectsInvalidPatchName) {
    BlockMeshParams p;
    p.patches[0].name = "1 starts with digit";
    EXPECT_FALSE(BlockMeshDict::generate(unitBox(), p).hasValue());
}

TEST(BlockMeshDict, DeterministicOutput) {
    auto a = BlockMeshDict::generate(unitBox(), BlockMeshParams{});
    auto b = BlockMeshDict::generate(unitBox(), BlockMeshParams{});
    ASSERT_TRUE(a.hasValue() && b.hasValue());
    EXPECT_EQ(a.value(), b.value());
}

TEST(BlockMeshDict, HonorsNonUniformSimpleGrading) {
    BlockMeshParams p = cavityParams();
    p.cells = {48, 96, 1};
    p.grading = {2.5, 0.4, 1.0};
    const auto dict = BlockMeshDict::generate(unitBox(), p);
    ASSERT_TRUE(dict.hasValue());
    EXPECT_NE(dict->find("(48 96 1) simpleGrading (2.5 0.4 1)"), std::string::npos);
}
TEST(BlockMeshDict, RejectsNonFiniteAndNonPositiveGrading) {
    BlockMeshParams p = cavityParams();
    p.grading = {1.0, 0.0, 1.0};
    EXPECT_FALSE(BlockMeshDict::generate(unitBox(), p).hasValue());
    p.grading = {1.0, -0.2, 1.0};
    EXPECT_FALSE(BlockMeshDict::generate(unitBox(), p).hasValue());
}
