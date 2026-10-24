// Integration test: CaseRunner orchestration against a FAKE OpenFOAM backend
// (shell-script doubles). This verifies process orchestration, PATH
// resolution, log capture and progress parsing ONLY. It is not CFD
// validation — see docs/VALIDATION.md.

#include <gtest/gtest.h>

#include "vortia/solver/openfoam/CaseRunner.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"

#include <filesystem>
#include <fstream>

using namespace vortia;
using namespace vortia::openfoam;

#ifndef _WIN32

namespace {

class FakeBackendTest : public ::testing::Test {
protected:
    void SetUp() override {
        dir_ = std::filesystem::temp_directory_path() /
               ("vortia-fake-" + std::to_string(std::hash<std::string>{}(
                                     ::testing::UnitTest::GetInstance()
                                         ->current_test_info()
                                         ->name())));
        std::filesystem::remove_all(dir_);
        bin_ = dir_ / "bin";
        std::filesystem::create_directories(bin_);
        environment_ = {{"PATH", bin_.string()}};

        Installation i;
        i.distribution = Distribution::Esi;
        i.version = Version::parse("v2606").value();
        caps_ = capabilitiesFor(i);
    }
    void TearDown() override { std::filesystem::remove_all(dir_); }

    void makeFakeTool(const std::string& name, const std::string& body) {
        const auto path = bin_ / name;
        {
            std::ofstream out(path);
            out << "#!/bin/sh\n" << body;
        }
        std::filesystem::permissions(path, std::filesystem::perms::owner_all);
    }

    static CaseSetup minimalSetup() {
        CaseSetup s;
        s.domain = BoxDomain::fromExtents({0, 0, 0}, {1, 1, 0.1}).value();
        s.mesh.cells = {10, 10, 1};
        s.mesh.patches = {{
            {BoxDomain::XMin, "walls", PatchKind::Wall},
            {BoxDomain::XMax, "walls2", PatchKind::Wall},
            {BoxDomain::YMin, "walls3", PatchKind::Wall},
            {BoxDomain::YMax, "lid", PatchKind::Wall},
            {BoxDomain::ZMin, "front", PatchKind::Empty},
            {BoxDomain::ZMax, "back", PatchKind::Empty},
        }};
        s.material.nu = KinematicViscosity(0.001);
        s.boundaries = {
            {"walls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
            {"walls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
            {"walls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
            {"lid", VelocityBc::FixedValue, {1, 0, 0}, PressureBc::ZeroGradient, 0.0},
            {"front", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
            {"back", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        };
        s.time = {0.0, 0.1, 0.001, 50, false, 1.0};
        return s;
    }

    std::filesystem::path dir_;
    std::filesystem::path bin_;
    std::vector<std::pair<std::string, std::string>> environment_;
    Capabilities caps_;
};

TEST_F(FakeBackendTest, FullPipelineWithScriptDoubles) {
    makeFakeTool("blockMesh", R"(
echo "Creating mesh"
mkdir -p constant/polyMesh
echo "Mesh created"
exit 0
)");
    makeFakeTool("checkMesh", "echo 'Mesh OK'\nexit 0\n");
    makeFakeTool("icoFoam", R"(
echo "Starting time loop"
echo "Time = 0.001"
echo "Courant Number mean: 0.02 max: 0.4"
echo "smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = 1e-06, No Iterations 4"
echo "DICPCG:  Solving for p, Initial residual = 0.9, Final residual = 8e-07, No Iterations 30"
echo "Time = 0.002"
echo "End"
exit 0
)");

    const auto caseDir = dir_ / "case";
    ASSERT_TRUE(CaseWriter::write(caseDir, minimalSetup(), caps_).hasValue());

    PosixProcessRunner processes;
    CaseRunner runner(processes, environment_);

    std::string log;
    RunCallbacks callbacks;
    callbacks.onOutput = [&](OutputStream, std::string_view chunk) { log += chunk; };

    auto mesh = runner.runMeshing(caseDir, caps_, callbacks, nullptr);
    ASSERT_TRUE(mesh.hasValue()) << mesh.error().describe();
    EXPECT_TRUE(mesh->process.succeeded());
    EXPECT_NE(log.find("Creating mesh"), std::string::npos);

    SolverProgress lastProgress;
    callbacks.onProgress = [&](const SolverProgress& p) { lastProgress = p; };
    auto solved = runner.runSolver(caseDir, minimalSetup(), caps_, callbacks, nullptr);
    ASSERT_TRUE(solved.hasValue()) << solved.error().describe();
    EXPECT_TRUE(solved->process.succeeded());
    EXPECT_FALSE(solved->diverged);
    EXPECT_DOUBLE_EQ(lastProgress.time, 0.002);
    EXPECT_EQ(lastProgress.residuals.size(), 2u); // Ux, p

    // The run log must have been persisted in the case.
    EXPECT_TRUE(std::filesystem::is_regular_file(caseDir / "log.vortia"));
}

TEST_F(FakeBackendTest, SolverFailureIsReportedNotSwallowed) {
    makeFakeTool("blockMesh", "echo ok\nexit 0\n");
    makeFakeTool("checkMesh", "echo ok\nexit 0\n");
    makeFakeTool("icoFoam", "echo 'cannot read file' 1>&2\nexit 2\n");

    const auto caseDir = dir_ / "case";
    ASSERT_TRUE(CaseWriter::write(caseDir, minimalSetup(), caps_).hasValue());

    PosixProcessRunner processes;
    CaseRunner runner(processes, environment_);
    auto mesh = runner.runMeshing(caseDir, caps_, {}, nullptr);
    ASSERT_TRUE(mesh.hasValue());

    auto solved = runner.runSolver(caseDir, minimalSetup(), caps_, {}, nullptr);
    ASSERT_TRUE(solved.hasValue());
    EXPECT_FALSE(solved->process.succeeded());
    EXPECT_EQ(solved->process.exitCode, 2);
    EXPECT_NE(solved->process.stdErr.find("cannot read file"), std::string::npos);
}

TEST_F(FakeBackendTest, MissingToolYieldsBackendUnavailable) {
    // No tools in PATH at all.
    PosixProcessRunner processes;
    CaseRunner runner(processes, environment_);
    auto mesh = runner.runMeshing(dir_, caps_, {}, nullptr);
    ASSERT_FALSE(mesh.hasValue());
    EXPECT_EQ(mesh.error().code, ErrorCode::BackendUnavailable);
}

TEST_F(FakeBackendTest, NozzleInitializationFailureIsReportedAndResumeKeepsFields) {
    makeFakeTool("blockMesh", "exit 0\n");
    makeFakeTool("checkMesh", "exit 0\n");
    makeFakeTool("setFields", "echo 'initialization failed'\nexit 3\n");
    const auto caseDir = dir_ / "case";
    ASSERT_TRUE(CaseWriter::write(caseDir, minimalSetup(), caps_).hasValue());
    { std::ofstream marker(caseDir / ".vortia-nozzle-case"); marker << "vortia-nozzle-video-2026-v1\n"; }
    PosixProcessRunner processes;
    CaseRunner runner(processes, environment_);
    auto first = runner.runMeshing(caseDir, caps_, {}, nullptr);
    ASSERT_TRUE(first.hasValue());
    EXPECT_EQ(first->name, "initialize nozzle");
    EXPECT_EQ(first->process.exitCode, 3);
    std::filesystem::create_directory(caseDir / "1e-06");
    auto resumed = runner.runMeshing(caseDir, caps_, {}, nullptr);
    ASSERT_TRUE(resumed.hasValue());
    EXPECT_TRUE(resumed->process.succeeded());
    EXPECT_NE(resumed->name, "initialize nozzle");
}

TEST_F(FakeBackendTest, DedicatedNozzleBackendBuildsOnceAndRequiresItsVersionAndMarker) {
    const auto caseDir=dir_/"case";
    ASSERT_TRUE(CaseWriter::write(caseDir,minimalSetup(),caps_).hasValue());
    std::filesystem::create_directory(caseDir/"solver");
    { std::ofstream source(caseDir/"solver"/"vortiaCentralFoam.C"); source << "fake backend source\n"; }
    { std::ofstream marker(caseDir/".vortia-nozzle-case"); marker << "vortia-nozzle-video-2026-v3\n"; }
    makeFakeTool("wmake",R"(
printf '#!/bin/sh\necho "Time = 50000"\necho "ExecutionTime = 2.6 s ClockTime = 3 s"\n' > "$VORTIA_NOZZLE_OUTPUT"
/bin/chmod +x "$VORTIA_NOZZLE_OUTPUT"
)");
    PosixProcessRunner processes;
    CaseRunner unsupported(processes,environment_);
    auto missingVersion=unsupported.runSolver(caseDir,minimalSetup(),caps_,{},nullptr,"vortiaCentralFoam");
    ASSERT_FALSE(missingVersion.hasValue());
    EXPECT_EQ(missingVersion.error().code,ErrorCode::BackendUnavailable);
    environment_.emplace_back("WM_PROJECT_VERSION","v2606");
    CaseRunner runner(processes,environment_);
    auto first=runner.runSolver(caseDir,minimalSetup(),caps_,{},nullptr,"vortiaCentralFoam");
    ASSERT_TRUE(first.hasValue());
    EXPECT_TRUE(first->process.succeeded());
    EXPECT_DOUBLE_EQ(first->finalTime,50000);
    makeFakeTool("wmake","exit 3\n");
    auto cached=runner.runSolver(caseDir,minimalSetup(),caps_,{},nullptr,"vortiaCentralFoam");
    ASSERT_TRUE(cached.hasValue());
    EXPECT_TRUE(cached->process.succeeded());
    for (const auto* kind : {"vortia-nozzle-video-2026-v1", "vortia-nozzle-video-2026-v2", "unmarked"}) {
        { std::ofstream marker(caseDir/".vortia-nozzle-case"); marker << kind << '\n'; }
        auto ordinary=runner.runSolver(caseDir,minimalSetup(),caps_,{},nullptr,"vortiaCentralFoam");
        ASSERT_FALSE(ordinary.hasValue());
        EXPECT_EQ(ordinary.error().code,ErrorCode::InvalidData);
    }
}

TEST_F(FakeBackendTest, DivergenceIsDetected) {
    makeFakeTool("blockMesh", "exit 0\n");
    makeFakeTool("checkMesh", "exit 0\n");
    makeFakeTool("icoFoam",
                 "echo 'smoothSolver:  Solving for Ux, Initial residual = nan, Final "
                 "residual = nan, No Iterations 999'\nexit 1\n");

    const auto caseDir = dir_ / "case";
    ASSERT_TRUE(CaseWriter::write(caseDir, minimalSetup(), caps_).hasValue());

    PosixProcessRunner processes;
    CaseRunner runner(processes, environment_);
    ASSERT_TRUE(runner.runMeshing(caseDir, caps_, {}, nullptr).hasValue());
    auto solved = runner.runSolver(caseDir, minimalSetup(), caps_, {}, nullptr);
    ASSERT_TRUE(solved.hasValue());
    EXPECT_TRUE(solved->diverged);
    EXPECT_FALSE(solved->divergenceReason.empty());
}

} // namespace

#endif // !_WIN32
