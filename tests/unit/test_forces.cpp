#include <gtest/gtest.h>

#include "vortia/solver/openfoam/Forces.hpp"

using namespace vortia;
using namespace vortia::openfoam;

TEST(ForcesFunctionObject, ListsPatchesDensityAndCentre) {
    ForcesRequest request;
    request.patches = {"bottomWall", "topWall"};
    request.rhoInf = 1.225;
    request.centreOfRotation = {0.25, 0.0, 0.0};
    const auto dict = forcesFunctionObject(request);
    ASSERT_TRUE(dict.hasValue()) << dict.error().describe();
    EXPECT_NE(dict->find("type            forces;"), std::string::npos);
    EXPECT_NE(dict->find("patches         (bottomWall topWall);"), std::string::npos);
    EXPECT_NE(dict->find("rhoInf          1.225;"), std::string::npos);
    EXPECT_NE(dict->find("CofR            (0.25 0 0);"), std::string::npos);
}

TEST(ForcesFunctionObject, RejectsMissingOrUnsafeInput) {
    ForcesRequest request;
    EXPECT_FALSE(forcesFunctionObject(request).hasValue()); // no patches
    request.patches = {"wall); system \"rm -rf\";"};
    EXPECT_FALSE(forcesFunctionObject(request).hasValue());
    request.patches = {"wall"};
    request.rhoInf = 0.0;
    EXPECT_FALSE(forcesFunctionObject(request).hasValue());
    request.rhoInf = 1.0;
    request.centreOfRotation.y = std::numeric_limits<double>::quiet_NaN();
    EXPECT_FALSE(forcesFunctionObject(request).hasValue());
}

TEST(ParseForces, ReadsModernEsiColumns) {
    const std::string text =
        "# Force\n"
        "# CofR                : (0 0 0)\n"
        "#\n"
        "# Time          \ttotal_x\ttotal_y\ttotal_z\tpressure_x\tpressure_y\tpressure_z\tviscous_x\tviscous_y\tviscous_z\n"
        "100\t1.0e-05\t2.0e-06\t0\t1.0e-06\t2.0e-06\t0\t9.0e-06\t0\t0\n"
        "200\t1.1e-05\t2.0e-06\t0\t1.0e-06\t2.0e-06\t0\t1.0e-05\t0\t0\n";
    const auto rows = parseForces(text);
    ASSERT_TRUE(rows.hasValue()) << rows.error().describe();
    ASSERT_EQ(rows->size(), 2U);
    EXPECT_DOUBLE_EQ(rows->back().time, 200.0);
    EXPECT_DOUBLE_EQ(rows->back().total.x, 1.1e-05);
    EXPECT_DOUBLE_EQ(rows->back().pressure.y, 2.0e-06);
    EXPECT_DOUBLE_EQ(rows->back().viscous.x, 1.0e-05);
}

TEST(ParseForces, ReadsLegacyBracketedRows) {
    const std::string text =
        "# Forces\n"
        "# Time       forces(pressure viscous porous) moment(pressure viscous porous)\n"
        "500 ((1 2 3) (0.5 0 -0.5) (0 0 0)) ((0 0 0) (0 0 0) (0 0 0))\n";
    const auto rows = parseForces(text);
    ASSERT_TRUE(rows.hasValue()) << rows.error().describe();
    ASSERT_EQ(rows->size(), 1U);
    EXPECT_DOUBLE_EQ(rows->front().pressure.z, 3.0);
    EXPECT_DOUBLE_EQ(rows->front().viscous.x, 0.5);
    EXPECT_DOUBLE_EQ(rows->front().total.x, 1.5);
    EXPECT_DOUBLE_EQ(rows->front().total.z, 2.5);
}

TEST(ParseForces, FailsOnEmptyTruncatedOrCorruptFiles) {
    EXPECT_FALSE(parseForces("").hasValue());
    EXPECT_FALSE(parseForces("# Time total_x total_y\n").hasValue());
    EXPECT_FALSE(parseForces("# Time total_x\n100 1 2 3\n").hasValue());      // too few columns
    EXPECT_FALSE(parseForces("# Time total_x\n100 1 2 3 4 5 6 7 8 nan\n").hasValue());
    EXPECT_FALSE(parseForces("100 (1 2 oops) (4 5 6)\n").hasValue());
}

TEST(Coefficients, MatchHandCalculation) {
    // q = 0.5 * 1.2 * 10^2 = 60 Pa; A = 0.5 m^2; F = (15, 30, 0) N.
    const FreeStream stream{1.2, 10.0, 0.5};
    EXPECT_DOUBLE_EQ(dynamicPressure(stream).value(), 60.0);
    const auto c = forceCoefficients({15.0, 30.0, 0.0}, {1, 0, 0}, {0, 1, 0}, stream);
    ASSERT_TRUE(c.hasValue());
    EXPECT_DOUBLE_EQ(c->drag, 0.5);
    EXPECT_DOUBLE_EQ(c->lift, 1.0);
    EXPECT_DOUBLE_EQ(pressureCoefficient(160.0, 100.0, stream).value(), 1.0); // stagnation
    EXPECT_DOUBLE_EQ(pressureCoefficient(100.0, 100.0, stream).value(), 0.0);
}

TEST(Coefficients, DirectionsAreNormalisedAndFollowAngleOfAttack) {
    const FreeStream stream{1.0, 2.0, 1.0}; // q A = 2
    const double a = 0.1;                   // rad
    const Vec3 drag{std::cos(a), std::sin(a), 0.0};
    const Vec3 lift{-std::sin(a), std::cos(a), 0.0};
    // A force purely along the wind axis is all drag, whatever the scale of the directions.
    const auto c = forceCoefficients(drag * 4.0, drag * 7.0, lift * 0.01, stream);
    ASSERT_TRUE(c.hasValue());
    EXPECT_NEAR(c->drag, 2.0, 1e-12);
    EXPECT_NEAR(c->lift, 0.0, 1e-12);
}

TEST(Coefficients, RejectInvalidReferences) {
    EXPECT_FALSE(dynamicPressure({0.0, 10.0, 1.0}).hasValue());
    EXPECT_FALSE(dynamicPressure({1.0, 0.0, 1.0}).hasValue());
    EXPECT_FALSE(forceCoefficients({1, 0, 0}, {1, 0, 0}, {0, 1, 0}, {1.0, 1.0, 0.0}).hasValue());
    EXPECT_FALSE(forceCoefficients({1, 0, 0}, {0, 0, 0}, {0, 1, 0}, {1.0, 1.0, 1.0}).hasValue());
    EXPECT_FALSE(forceCoefficients({1, 0, 0}, {1, 0, 0}, {1, 1, 0}, {1.0, 1.0, 1.0}).hasValue());
    EXPECT_FALSE(pressureCoefficient(std::numeric_limits<double>::infinity(), 0.0, {1.0, 1.0, 1.0}).hasValue());
}
