#include <gtest/gtest.h>

#include "vortia/solver/openfoam/CaseWriter.hpp"

#include <filesystem>
#include <fstream>
#include <sstream>

using namespace vortia;
using namespace vortia::openfoam;

namespace {

/// 2D lid-driven cavity (Re = 100), mirroring the classic icoFoam tutorial.
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
    s.material.nu = KinematicViscosity(0.001);
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

Capabilities esiCaps() {
    Installation i;
    i.distribution = Distribution::Esi;
    i.version = Version::parse("v2606").value();
    return capabilitiesFor(i);
}

class CaseWriterTest : public ::testing::Test {
protected:
    void SetUp() override {
        dir_ = std::filesystem::temp_directory_path() /
               ("vortia-case-" + std::to_string(std::hash<std::string>{}(
                                     ::testing::UnitTest::GetInstance()
                                         ->current_test_info()
                                         ->name())));
        std::filesystem::remove_all(dir_);
    }
    void TearDown() override { std::filesystem::remove_all(dir_); }

    std::string readFile(const std::filesystem::path& p) {
        std::ostringstream ss;
        ss << std::ifstream(p).rdbuf();
        return ss.str();
    }

    std::filesystem::path dir_;
};

TEST_F(CaseWriterTest, ControlDictMatchesSolver) {
    const std::string cd = CaseWriter::controlDict(cavitySetup(), esiCaps());
    EXPECT_NE(cd.find("application     icoFoam;"), std::string::npos);
    EXPECT_NE(cd.find("endTime         0.5;"), std::string::npos);
    EXPECT_NE(cd.find("deltaT          0.001;"), std::string::npos);
}

TEST_F(CaseWriterTest, SteadyControlDictUsesIterations) {
    auto s = cavitySetup();
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    const std::string cd = CaseWriter::controlDict(s, esiCaps());
    EXPECT_NE(cd.find("application     simpleFoam;"), std::string::npos);
    EXPECT_NE(cd.find("deltaT          1;"), std::string::npos);
}

TEST_F(CaseWriterTest, FieldUHasCavityBoundaryConditions) {
    const std::string u = CaseWriter::fieldU(cavitySetup());
    EXPECT_NE(u.find("class       volVectorField;"), std::string::npos);
    EXPECT_NE(u.find("dimensions      [0 1 -1 0 0 0 0];"), std::string::npos);
    EXPECT_NE(u.find("movingWall\n    {\n        type            fixedValue;\n"
                     "        value           uniform (1 0 0);"),
              std::string::npos);
    EXPECT_NE(u.find("fixedWalls\n    {\n        type            noSlip;"), std::string::npos);
    EXPECT_NE(u.find("frontAndBack\n    {\n        type            empty;"), std::string::npos);
}

TEST_F(CaseWriterTest, FieldPHasKinematicDimensions) {
    const std::string p = CaseWriter::fieldP(cavitySetup());
    EXPECT_NE(p.find("dimensions      [0 2 -2 0 0 0 0];"), std::string::npos);
    EXPECT_NE(p.find("movingWall\n    {\n        type            zeroGradient;"),
              std::string::npos);
}

TEST_F(CaseWriterTest, TransportPropertiesCarriesNu) {
    const std::string tp = CaseWriter::transportProperties(cavitySetup(), esiCaps());
    EXPECT_NE(tp.find("transportModel  Newtonian;"), std::string::npos);
    EXPECT_NE(tp.find("nu              [0 2 -1 0 0 0 0] 0.001;"), std::string::npos);
}

TEST_F(CaseWriterTest, FoundationUsesFoamRunAndPhysicalProperties) {
    Installation f;
    f.distribution = Distribution::Foundation;
    f.version = Version::parse("12").value();
    const auto caps = capabilitiesFor(f);
    const std::string cd = CaseWriter::controlDict(cavitySetup(), caps);
    EXPECT_NE(cd.find("application     foamRun;"), std::string::npos);
    const std::string tp = CaseWriter::transportProperties(cavitySetup(), caps);
    EXPECT_NE(tp.find("object      physicalProperties;"), std::string::npos);
}

TEST_F(CaseWriterTest, WritesCompleteCaseTree) {
    auto written = CaseWriter::write(dir_ / "case", cavitySetup(), esiCaps());
    ASSERT_TRUE(written.hasValue()) << written.error().describe();
    for (const char* rel : {"system/controlDict", "system/fvSchemes", "system/fvSolution",
                            "system/blockMeshDict", "constant/transportProperties", "0/U",
                            "0/p", ".vortia-case"}) {
        EXPECT_TRUE(std::filesystem::is_regular_file(dir_ / "case" / rel)) << rel;
    }
}

TEST_F(CaseWriterTest, RefusesForeignDirectory) {
    std::filesystem::create_directories(dir_ / "case" / "system");
    std::ofstream(dir_ / "case" / "system" / "controlDict") << "// user case, not ours";
    auto written = CaseWriter::write(dir_ / "case", cavitySetup(), esiCaps());
    ASSERT_FALSE(written.hasValue());
    EXPECT_EQ(written.error().code, ErrorCode::PermissionDenied);
    // Foreign file must be intact.
    EXPECT_EQ(readFile(dir_ / "case" / "system" / "controlDict"), "// user case, not ours");
}

TEST_F(CaseWriterTest, RefusesOverwriteWithoutFlag) {
    ASSERT_TRUE(CaseWriter::write(dir_ / "case", cavitySetup(), esiCaps()).hasValue());
    auto second = CaseWriter::write(dir_ / "case", cavitySetup(), esiCaps());
    ASSERT_FALSE(second.hasValue());
    auto third = CaseWriter::write(dir_ / "case", cavitySetup(), esiCaps(), {.overwrite = true});
    EXPECT_TRUE(third.hasValue());
}

TEST_F(CaseWriterTest, RejectsInvalidSetup) {
    auto s = cavitySetup();
    s.boundaries.clear(); // no BCs at all → hard error
    auto written = CaseWriter::write(dir_ / "case", s, esiCaps());
    EXPECT_FALSE(written.hasValue());
}

TEST_F(CaseWriterTest, SteadyCaseIncludesTurbulenceProperties) {
    auto s = cavitySetup();
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    auto written = CaseWriter::write(dir_ / "case", s, esiCaps());
    ASSERT_TRUE(written.hasValue());
    const std::string turb = readFile(dir_ / "case" / "constant" / "turbulenceProperties");
    EXPECT_NE(turb.find("simulationType  laminar;"), std::string::npos);
}

} // namespace
