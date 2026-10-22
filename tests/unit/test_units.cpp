#include <gtest/gtest.h>

#include "vortia/core/Units.hpp"

using namespace vortia;
using namespace vortia::literals;

TEST(Units, SameDimensionArithmetic) {
    const Length a(2.0);
    const Length b = 3.0_m;
    EXPECT_DOUBLE_EQ((a + b).siValue(), 5.0);
    EXPECT_DOUBLE_EQ((a - b).siValue(), -1.0);
    EXPECT_DOUBLE_EQ((b / a), 1.5); // dimensionless ratio
    EXPECT_TRUE(b > a);
}

TEST(Units, DimensionalMultiplication) {
    const Velocity v = 2.0_mps;
    const Time t = 3.0_s;
    const Length d = v * t;
    EXPECT_DOUBLE_EQ(d.siValue(), 6.0);
}

TEST(Units, VelocityFromLengthOverTime) {
    const Length d = 10.0_m;
    const Time t = 4.0_s;
    const Velocity v = d / t;
    EXPECT_DOUBLE_EQ(v.siValue(), 2.5);
}

TEST(Units, KinematicViscosityDimensions) {
    // nu = mu / rho : (kg m^-1 s^-1) / (kg m^-3) = m^2 s^-1
    const DynamicViscosity mu(1.8e-5);
    const Density rho(1.2);
    const KinematicViscosity nu = mu / rho;
    EXPECT_NEAR(nu.siValue(), 1.5e-5, 1e-12);
}

TEST(Units, ScalarScaling) {
    const Length a = 1.0_m * 2.5;
    EXPECT_DOUBLE_EQ(a.siValue(), 2.5);
    const Length b = a / 2.0;
    EXPECT_DOUBLE_EQ(b.siValue(), 1.25);
}

TEST(Units, ReynoldsNumberIsDimensionless) {
    const Velocity u = 1.0_mps;
    const Length l = 0.1_m;
    const KinematicViscosity nu(1.5e-5);
    // u·l has viscosity dimensions; dividing by nu yields a plain ratio.
    const KinematicViscosity ul = u * l;
    static_assert(std::is_same_v<decltype(ul), const KinematicViscosity>);
    const double re = ul / nu;
    EXPECT_NEAR(re, 6666.6667, 1e-3);
}
