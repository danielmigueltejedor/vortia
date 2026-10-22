#include <gtest/gtest.h>

#include "vortia/solver/openfoam/LogParser.hpp"

using namespace vortia::openfoam;

TEST(LogParser, ParsesTimeSteps) {
    LogParser parser;
    parser.feed("Create time\n\nTime = 0.001\n\nTime = 0.002\n");
    EXPECT_DOUBLE_EQ(parser.progress().time, 0.002);
}

TEST(LogParser, ParsesResiduals) {
    LogParser parser;
    parser.feed("smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = "
                "2.96335e-06, No Iterations 19\n");
    parser.feed("DICPCG:  Solving for p, Initial residual = 0.5, Final residual = "
                "4.1e-07, No Iterations 35\n");
    const auto& residuals = parser.progress().residuals;
    ASSERT_EQ(residuals.size(), 2u);
    EXPECT_EQ(residuals[0].field, "Ux");
    EXPECT_DOUBLE_EQ(residuals[0].initial, 1.0);
    EXPECT_NEAR(residuals[0].final, 2.96335e-06, 1e-12);
    EXPECT_EQ(residuals[0].iterations, 19);
    EXPECT_EQ(residuals[1].field, "p");
}

TEST(LogParser, ParsesCourantNumber) {
    LogParser parser;
    parser.feed("Courant Number mean: 0.0292217 max: 0.585431\n");
    EXPECT_DOUBLE_EQ(parser.progress().courantMean, 0.0292217);
    EXPECT_DOUBLE_EQ(parser.progress().courantMax, 0.585431);
}

TEST(LogParser, HandlesSplitLinesAcrossChunks) {
    LogParser parser;
    parser.feed("Time = 0.0");
    parser.feed("05\nsmoothSolver:  Solving for Uy, Initial resi");
    parser.feed("dual = 0.2, Final residual = 1e-05, No Iterations 3\n");
    EXPECT_DOUBLE_EQ(parser.progress().time, 0.005);
    ASSERT_EQ(parser.progress().residuals.size(), 1u);
    EXPECT_EQ(parser.progress().residuals[0].field, "Uy");
}

TEST(LogParser, DetectsNanDivergence) {
    LogParser parser;
    parser.feed("smoothSolver:  Solving for Ux, Initial residual = nan, Final residual = "
                "nan, No Iterations 1000\n");
    EXPECT_TRUE(parser.progress().diverged);
    EXPECT_NE(parser.progress().divergenceReason.find("Ux"), std::string::npos);
}

TEST(LogParser, DetectsFoamFatal) {
    LogParser parser;
    parser.feed("--> FOAM FATAL ERROR: \n");
    EXPECT_TRUE(parser.progress().diverged);
}

TEST(LogParser, KeepsLatestSamplePerField) {
    LogParser parser;
    parser.feed("smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = "
                "1e-03, No Iterations 2\n");
    parser.feed("smoothSolver:  Solving for Ux, Initial residual = 0.1, Final residual = "
                "1e-06, No Iterations 4\n");
    ASSERT_EQ(parser.progress().residuals.size(), 1u);
    EXPECT_DOUBLE_EQ(parser.progress().residuals[0].initial, 0.1);
}

TEST(LogParser, ResetClearsState) {
    LogParser parser;
    parser.feed("Time = 0.5\n");
    parser.reset();
    EXPECT_DOUBLE_EQ(parser.progress().time, 0.0);
    EXPECT_TRUE(parser.progress().residuals.empty());
}

TEST(LogParser, FlushCapturesTrailingLineWithoutNewline) {
    LogParser parser;
    parser.feed("Time = 0.5");
    EXPECT_DOUBLE_EQ(parser.progress().time, 0.0); // not yet processed
    parser.flush();
    EXPECT_DOUBLE_EQ(parser.progress().time, 0.5);
}
