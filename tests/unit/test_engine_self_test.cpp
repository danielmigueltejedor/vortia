#include <gtest/gtest.h>

#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/solver/openfoam/EngineSelfTest.hpp"
#include "PrivateTempDir.hpp"

#include <cstdlib>
#include <filesystem>
#include <fstream>

using namespace vortia;
using namespace vortia::openfoam;

namespace {

class EngineSelfTest : public ::testing::Test {
protected:
    void SetUp() override { dir_ = vortia::test::makePrivateTempDir("vortia-selftest-"); }
    void TearDown() override { std::filesystem::remove_all(dir_); }
    std::filesystem::path dir_;
};

TEST_F(EngineSelfTest, RejectsProcessCountsOutsideTwoToEight) {
    PosixProcessRunner runner;
    Installation installation;
    for (const int processes : {0, 1, 9}) {
        const auto report = runEngineSelfTest(installation, runner, dir_, processes);
        ASSERT_FALSE(report.hasValue());
        EXPECT_EQ(report.error().code, ErrorCode::InvalidArgument);
    }
}

TEST_F(EngineSelfTest, RefusesAWorkDirectoryThatIsNotEmpty) {
    std::ofstream(dir_ / "existing-results.txt") << "keep me\n";
    PosixProcessRunner runner;
    Installation installation;
    const auto report = runEngineSelfTest(installation, runner, dir_);
    ASSERT_FALSE(report.hasValue());
    EXPECT_EQ(report.error().code, ErrorCode::InvalidArgument);
    EXPECT_TRUE(std::filesystem::exists(dir_ / "existing-results.txt"));
}

TEST_F(EngineSelfTest, FailsCleanlyWhenTheEngineEnvironmentCannotBeLoaded) {
    PosixProcessRunner runner;
    Installation installation;
    installation.rootDir = dir_ / "missing-engine";
    installation.bashrc = installation.rootDir / "etc" / "bashrc";
    const auto work = dir_ / "work";
    std::filesystem::create_directories(work);
    const auto report = runEngineSelfTest(installation, runner, work);
    ASSERT_FALSE(report.hasValue());
    EXPECT_TRUE(std::filesystem::is_empty(work));
}

// Real engine: serial + two-process run. Enabled by VORTIA_NATIVE_OF_ROOT.
TEST_F(EngineSelfTest, NativeEngineRunsSerialAndParallel) {
    const char* nativeRoot = std::getenv("VORTIA_NATIVE_OF_ROOT");
    if (!nativeRoot) GTEST_SKIP() << "Set VORTIA_NATIVE_OF_ROOT to run against a real engine";
    PosixProcessRunner runner;
    const auto installation = Detector::probe(nativeRoot);
    ASSERT_TRUE(installation.hasValue()) << installation.error().describe();
    const auto report = runEngineSelfTest(*installation, runner, dir_);
    ASSERT_TRUE(report.hasValue()) << report.error().describe();
    EXPECT_EQ(report->steps.size(), 7U);
}

} // namespace
