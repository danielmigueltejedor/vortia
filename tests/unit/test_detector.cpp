#include <gtest/gtest.h>

#include "vortia/solver/openfoam/Detector.hpp"

#include <filesystem>
#include <fstream>

using namespace vortia;
using namespace vortia::openfoam;

namespace {

class DetectorTest : public ::testing::Test {
protected:
    void SetUp() override {
        dir_ = std::filesystem::temp_directory_path() /
               ("vortia-foam-" + std::to_string(std::hash<std::string>{}(
                                     ::testing::UnitTest::GetInstance()
                                         ->current_test_info()
                                         ->name())));
        std::filesystem::remove_all(dir_);
    }
    void TearDown() override { std::filesystem::remove_all(dir_); }

    /// Builds a minimal fake OpenFOAM root: <root>/etc/bashrc with a version.
    std::filesystem::path fakeRoot(const std::string& name, const std::string& versionLine) {
        const auto root = dir_ / name;
        std::filesystem::create_directories(root / "etc");
        std::ofstream(root / "etc" / "bashrc")
            << "# fake OpenFOAM bashrc for tests\n"
            << "export WM_PROJECT=OpenFOAM\n"
            << versionLine << "\n";
        return root;
    }

    std::filesystem::path dir_;
};

TEST_F(DetectorTest, ProbesEsiInstallation) {
    const auto root = fakeRoot("OpenFOAM-v2606", "export WM_PROJECT_VERSION=v2606");
    auto installation = Detector::probe(root);
    ASSERT_TRUE(installation.hasValue()) << installation.error().describe();
    EXPECT_EQ(installation->distribution, Distribution::Esi);
    EXPECT_EQ(installation->version.major, 2606);
}

TEST_F(DetectorTest, ProbesFoundationInstallation) {
    const auto root = fakeRoot("OpenFOAM-14", "WM_PROJECT_VERSION=14");
    auto installation = Detector::probe(root);
    ASSERT_TRUE(installation.hasValue());
    EXPECT_EQ(installation->distribution, Distribution::Foundation);
    EXPECT_EQ(installation->version.major, 14);
}

TEST_F(DetectorTest, RejectsDirectoryWithoutBashrc) {
    const auto root = dir_ / "not-foam";
    std::filesystem::create_directories(root);
    auto installation = Detector::probe(root);
    ASSERT_FALSE(installation.hasValue());
    EXPECT_EQ(installation.error().code, ErrorCode::NotFound);
}

TEST_F(DetectorTest, RejectsBashrcWithoutVersion) {
    const auto root = dir_ / "OpenFOAM-broken";
    std::filesystem::create_directories(root / "etc");
    std::ofstream(root / "etc" / "bashrc") << "# nothing useful here\n";
    auto installation = Detector::probe(root);
    ASSERT_FALSE(installation.hasValue());
    EXPECT_EQ(installation.error().code, ErrorCode::ParseError);
}

TEST_F(DetectorTest, ResolveExecutableSearchesPath) {
    // A fake executable inside a temp bin dir.
    const auto bin = dir_ / "bin";
    std::filesystem::create_directories(bin);
    const auto exe = bin / "blockMesh";
    {
        std::ofstream(exe) << "#!/bin/sh\nexit 0\n";
        std::filesystem::permissions(exe, std::filesystem::perms::owner_all);
    }
    std::vector<std::pair<std::string, std::string>> env = {{"PATH", bin.string()}};
    auto resolved = Detector::resolveExecutable("blockMesh", env);
    ASSERT_TRUE(resolved.hasValue());
    EXPECT_EQ(resolved.value(), exe);
    EXPECT_FALSE(Detector::resolveExecutable("missingTool", env).hasValue());
}

TEST_F(DetectorTest, ResolveExecutableRejectsPathTraversal) {
    std::vector<std::pair<std::string, std::string>> env = {{"PATH", dir_.string()}};
    EXPECT_FALSE(Detector::resolveExecutable("../evil", env).hasValue());
    EXPECT_FALSE(Detector::resolveExecutable("/abs/path", env).hasValue());
    EXPECT_FALSE(Detector::resolveExecutable("", env).hasValue());
}

} // namespace
