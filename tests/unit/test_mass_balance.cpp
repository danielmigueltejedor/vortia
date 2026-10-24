#include "vortia/solver/openfoam/MassBalance.hpp"

#include <filesystem>
#include <fstream>
#include <chrono>
#include <gtest/gtest.h>

namespace {

using vortia::openfoam::isNozzlePracticeCase;
using vortia::openfoam::nozzleMassBalance;

} // namespace

TEST(MassBalance, EmptyCaseIsNotNozzle) {
    EXPECT_FALSE(isNozzlePracticeCase({}));
    const auto result = nozzleMassBalance({});
    EXPECT_FALSE(result.available);
    EXPECT_EQ(result.samples, 0);
}

TEST(MassBalance, NonexistentDirectoryReturnsUnavailable) {
    const auto result = nozzleMassBalance("/tmp/vortia-nonexistent-case-12345");
    EXPECT_FALSE(result.available);
}

namespace {
class MassWindow : public ::testing::Test {
protected:
    std::filesystem::path root;
    void SetUp() override {
        root=std::filesystem::temp_directory_path()/
            ("vortia-window-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
        std::filesystem::create_directories(root);
    }
    void TearDown() override {std::filesystem::remove_all(root);}
    void write(const std::string& channel,const std::string& start,const std::string& data) {
        const auto dir=root/"postProcessing"/channel/start;std::filesystem::create_directories(dir);
        std::ofstream(dir/"surfaceFieldValue.dat")<<"# Time sum(phi)\n"<<data;
    }
    void balanced() {
        write("mass","0","0 -1\n500 -1\n1000 -1\n");
        write("massOut","0","0 2\n500 2\n1000 2\n");
        write("massFar","0","0 -1\n500 -1\n1000 -1\n");
    }
};
TEST_F(MassWindow, AGoodFinalSampleCannotHideEarlierOscillations) {
    balanced();write("massOut","0","0 2.2\n500 1.8\n1000 2\n");
    const auto r=nozzleMassBalance(root);ASSERT_TRUE(r.available)<<r.reason;
    EXPECT_NEAR(r.meanAbsolutePercent,40.0/3.0,1e-9);EXPECT_NEAR(r.maximumAbsolutePercent,20,1e-9);
}
TEST_F(MassWindow, RestartsSupersedeAbandonedFutureData) {
    balanced();write("mass","500","500 -1\n");write("massOut","500","500 2\n");write("massFar","500","500 -1\n");
    const auto r=nozzleMassBalance(root);EXPECT_FALSE(r.available);EXPECT_DOUBLE_EQ(r.lastIteration,500);
}
TEST_F(MassWindow, UnalignedOrInvalidFlowsDoNotProduceFavourableResults) {
    balanced();write("massFar","0","0 -1\n1000 -1\n");EXPECT_FALSE(nozzleMassBalance(root).available);
    for(const auto& flow:{"nan","inf","0","1","bad"}) {
        balanced();write("mass","0",std::string("0 -1\n500 ")+flow+"\n1000 -1\n");
        EXPECT_FALSE(nozzleMassBalance(root).available)<<flow;
    }
}
TEST_F(MassWindow, OnlyExactCaseMarkersSelectTheNozzleAudit) {
    std::filesystem::create_directories(root/"0");std::filesystem::create_directories(root/"system");
    std::ofstream(root/"0/U")<<"field";std::ofstream(root/"0/p")<<"field";
    std::ofstream(root/"system/controlDict")<<"application icoFoam;";
    EXPECT_FALSE(isNozzlePracticeCase(root));
    std::ofstream(root/".vortia-nozzle-case")<<"vortia-nozzle-video-2026-v3\n";
    EXPECT_TRUE(isNozzlePracticeCase(root));
}
}
