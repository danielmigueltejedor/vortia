#include <gtest/gtest.h>

#include "vortia/core/Project.hpp"

#include <filesystem>
#include <fstream>
#include <vector>

using namespace vortia;

namespace {

class ProjectTest : public ::testing::Test {
protected:
    void SetUp() override {
        root_ = std::filesystem::temp_directory_path() /
                ("vortia-test-" + std::to_string(
                     std::hash<std::string>{}(::testing::UnitTest::GetInstance()
                                                  ->current_test_info()
                                                  ->name())));
        std::filesystem::remove_all(root_);
    }
    void TearDown() override { std::filesystem::remove_all(root_); }

    std::filesystem::path root_;
};

TEST_F(ProjectTest, CreatePersistsManifest) {
    auto project = Project::create(root_, "cavity");
    ASSERT_TRUE(project.hasValue());
    EXPECT_TRUE(std::filesystem::is_regular_file(project->manifestPath()));
    EXPECT_EQ(project->name(), "cavity");
    EXPECT_EQ(project->caseState(), Project::CaseState::NotCreated);
}

TEST_F(ProjectTest, OpenRoundTrip) {
    {
        auto project = Project::create(root_, "cavity");
        ASSERT_TRUE(project.hasValue());
        project->setBackendFingerprint("esi/v2606@/opt/OpenFOAM-v2606");
        project->setSetupJson(R"({"geometry":{"kind":"box"}})");
        project->setCaseState(Project::CaseState::Meshed);
        ASSERT_TRUE(project->save().hasValue());
    }
    auto reopened = Project::open(root_);
    ASSERT_TRUE(reopened.hasValue());
    EXPECT_EQ(reopened->name(), "cavity");
    EXPECT_EQ(reopened->backendFingerprint(), "esi/v2606@/opt/OpenFOAM-v2606");
    EXPECT_EQ(reopened->caseState(), Project::CaseState::Meshed);
    EXPECT_NE(reopened->setupJson().find("box"), std::string::npos);
}

TEST_F(ProjectTest, RefusesNonEmptyDirectory) {
    std::filesystem::create_directories(root_);
    std::ofstream(root_ / "precious.txt") << "do not touch";
    auto project = Project::create(root_, "cavity");
    EXPECT_FALSE(project.hasValue());
    // The foreign file must still be there and untouched.
    EXPECT_TRUE(std::filesystem::is_regular_file(root_ / "precious.txt"));
}

TEST_F(ProjectTest, DetectsCorruptedManifest) {
    auto project = Project::create(root_, "cavity");
    ASSERT_TRUE(project.hasValue());
    { std::ofstream(project->manifestPath()) << "{ this is not json"; }
    auto reopened = Project::open(root_);
    ASSERT_FALSE(reopened.hasValue());
    EXPECT_EQ(reopened.error().code, ErrorCode::ParseError);
}

TEST_F(ProjectTest, RejectsUnsupportedSchema) {
    auto project = Project::create(root_, "cavity");
    ASSERT_TRUE(project.hasValue());
    {
        std::ofstream(project->manifestPath())
            << R"({"schemaVersion": 9999, "name": "future"})";
    }
    auto reopened = Project::open(root_);
    ASSERT_FALSE(reopened.hasValue());
    EXPECT_EQ(reopened.error().code, ErrorCode::UnsupportedVersion);
}

TEST_F(ProjectTest, RejectsWrongManifestTypesWithoutThrowing) {
    auto project = Project::create(root_, "cavity");
    ASSERT_TRUE(project);
    const std::vector<std::string> contents{
        "[]", "null", R"({"schemaVersion":"1","name":"test"})",
        R"({"schemaVersion":1,"name":[]})",
        R"({"schemaVersion":1,"name":"test","backendFingerprint":42})",
        R"({"schemaVersion":1,"name":"test","caseState":false})"
    };
    for (const auto& content : contents) {
        SCOPED_TRACE(content);
        std::ofstream(project->manifestPath()) << content;
        const auto opened = Project::open(root_);
        ASSERT_FALSE(opened);
        EXPECT_EQ(opened.error().code, ErrorCode::InvalidData);
    }
}

TEST_F(ProjectTest, OpenFailsOnMissingManifest) {
    std::filesystem::create_directories(root_);
    auto reopened = Project::open(root_);
    ASSERT_FALSE(reopened.hasValue());
    EXPECT_EQ(reopened.error().code, ErrorCode::NotFound);
}

TEST_F(ProjectTest, RejectsEmptyName) {
    auto project = Project::create(root_, "");
    EXPECT_FALSE(project.hasValue());
}

} // namespace
