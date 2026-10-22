#include <gtest/gtest.h>

#include "vortia/physics/Physics.hpp"

using namespace vortia;

namespace {

/// Valid 2D lid-driven cavity: movingWall on ymax, walls elsewhere,
/// empty on z faces — mirroring the classic OpenFOAM tutorial.
CaseSetup cavitySetup() {
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
    s.material.nu = KinematicViscosity(0.001); // Re = U·L/nu = 1·0.1/0.001 = 100
    s.boundaries = {
        {"fixedWalls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"movingWall", VelocityBc::FixedValue, {1.0, 0.0, 0.0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.time = {0.0, 0.5, 0.001, 100, false, 1.0};
    s.solver = SolverApp::IcoFoam;
    return s;
}

bool hasError(const std::vector<Issue>& issues, const std::string& fragment) {
    for (const auto& i : issues)
        if (i.severity == Severity::Error && i.message.find(fragment) != std::string::npos)
            return true;
    return false;
}

bool hasWarning(const std::vector<Issue>& issues, const std::string& fragment) {
    for (const auto& i : issues)
        if (i.severity == Severity::Warning && i.message.find(fragment) != std::string::npos)
            return true;
    return false;
}

} // namespace

TEST(Physics, CavitySetupHasNoErrors) {
    const auto issues = cavitySetup().validate();
    for (const auto& i : issues)
        EXPECT_NE(i.severity, Severity::Error) << i.message;
}

TEST(Physics, ReynoldsEstimateMatchesAnalytic) {
    EXPECT_NEAR(cavitySetup().estimateReynolds(), 100.0, 1e-9);
}

TEST(Physics, CourantEstimate) {
    // U=1 m/s, deltaT=0.001, min cell = 0.1/20 = 0.005 (x,y) → Co = 0.2
    EXPECT_NEAR(cavitySetup().estimateMaxCourant(), 0.2, 1e-9);
}

TEST(Physics, WarnsOnHighCourant) {
    auto s = cavitySetup();
    s.time.deltaT = 0.02; // Co = 2 → warning
    EXPECT_TRUE(hasWarning(s.validate(), "Courant"));
}

TEST(Physics, ErrorOnMissingBoundary) {
    auto s = cavitySetup();
    s.boundaries.pop_back();
    EXPECT_TRUE(hasError(s.validate(), "frontAndBack2"));
}

TEST(Physics, ErrorOnBCPatchMismatch) {
    auto s = cavitySetup();
    s.boundaries[0].patchName = "ghost";
    EXPECT_TRUE(hasError(s.validate(), "unknown patch 'ghost'"));
}

TEST(Physics, ErrorOnEmptyPatchWithWallBc) {
    auto s = cavitySetup();
    s.boundaries[4].velocity = VelocityBc::NoSlip; // frontAndBack must stay empty
    EXPECT_TRUE(hasError(s.validate(), "'empty'"));
}

TEST(Physics, ErrorOnInvalidViscosity) {
    auto s = cavitySetup();
    s.material.nu = KinematicViscosity(0.0);
    EXPECT_TRUE(hasError(s.validate(), "viscosity"));
}

TEST(Physics, ErrorOnSolverTimeSchemeMismatch) {
    auto s = cavitySetup();
    s.solver = SolverApp::SimpleFoam; // but ddt is Euler (transient)
    EXPECT_TRUE(hasError(s.validate(), "simpleFoam"));
}

TEST(Physics, SteadySimpleFoamIsConsistent) {
    auto s = cavitySetup();
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    const auto issues = s.validate();
    for (const auto& i : issues)
        EXPECT_NE(i.severity, Severity::Error) << i.message;
}
