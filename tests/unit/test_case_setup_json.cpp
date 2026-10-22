#include <gtest/gtest.h>

#include "vortia/physics/CaseSetupJson.hpp"
#include "vortia/physics/Physics.hpp"

using namespace vortia;

namespace {

CaseSetup cavity() {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {0.1, 0.1, 0.01}).value();
    s.mesh.cells = {20, 20, 1};
    s.mesh.patches = {{
        {BoxDomain::XMin, "fixedWalls", PatchKind::Wall},
        {BoxDomain::XMax, "fixedWalls2", PatchKind::Wall},
        {BoxDomain::YMin, "fixedWalls3", PatchKind::Wall},
        {BoxDomain::YMax, "movingWall", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.nu = KinematicViscosity(0.001);
    s.boundaries = {
        {"fixedWalls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"movingWall", VelocityBc::FixedValue, {1.0, 0.0, 0.0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"fixedWalls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
    };
    s.time = {0.0, 0.5, 0.001, 100, false, 1.0};
    s.solver = SolverApp::IcoFoam;
    return s;
}

} // namespace

TEST(CaseSetupJson, RoundTripPreservesSetup) {
    const CaseSetup original = cavity();
    const std::string json = caseSetupToJson(original);
    auto loaded = caseSetupFromJson(json);
    ASSERT_TRUE(loaded.hasValue()) << loaded.error().describe();

    EXPECT_EQ(loaded->domain.min(), original.domain.min());
    EXPECT_EQ(loaded->domain.max(), original.domain.max());
    EXPECT_EQ(loaded->mesh.cells, original.mesh.cells);
    EXPECT_DOUBLE_EQ(loaded->material.nu.siValue(), original.material.nu.siValue());
    EXPECT_DOUBLE_EQ(loaded->time.endTime, original.time.endTime);
    EXPECT_DOUBLE_EQ(loaded->time.deltaT, original.time.deltaT);
    EXPECT_EQ(loaded->solver, original.solver);
    ASSERT_EQ(loaded->boundaries.size(), original.boundaries.size());
}

TEST(CaseSetupJson, RejectsMalformedJson) {
    EXPECT_FALSE(caseSetupFromJson("{ not json").hasValue());
}

TEST(CaseSetupJson, RejectsUnknownEnum) {
    std::string json = caseSetupToJson(cavity());
    // Corrupt a solver enum value.
    // Simple string replace of the solver token.
    const std::string from = "\"solver\": \"icoFoam\"";
    const std::string to = "\"solver\": \"totallyMadeUp\"";
    const auto pos = json.find(from);
    ASSERT_NE(pos, std::string::npos);
    json.replace(pos, from.size(), to);
    EXPECT_FALSE(caseSetupFromJson(json).hasValue());
}

TEST(CaseSetupJson, RejectsInvalidSetup) {
    auto s = cavity();
    s.boundaries.clear(); // no BCs → invalid
    // Build JSON manually from a valid setup then corrupt? Instead, generate
    // from a valid setup and verify loading a valid one succeeds; invalid
    // setups never reach JSON because CaseWriter rejects them first. Here we
    // check that a loaded setup with mismatched BCs is rejected.
    auto valid = cavity();
    // Remove one boundary from the JSON by editing
    std::string json = caseSetupToJson(valid);
    // Drop the last boundary entry crudely: replace "],\n  \"time\"" etc.
    // Simpler: just ensure malformed boundaries fail.
    const std::string needle = "\"boundaries\": [";
    ASSERT_NE(json.find(needle), std::string::npos);
    // Inject a boundary referencing an unknown patch
    std::string bad = json;
    const auto insertPos = bad.find(needle) + needle.size();
    bad.insert(insertPos,
               R"({"patch":"ghost","velocity":"noSlip","velocityValue":[0,0,0],"pressure":"zeroGradient","pressureValue":0},)");
    EXPECT_FALSE(caseSetupFromJson(bad).hasValue());
}