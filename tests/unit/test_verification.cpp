#include <gtest/gtest.h>

#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"
#include "vortia/verification/ProfileMetrics.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"

#include <cmath>
#include <vector>

using namespace vortia;
using namespace vortia::verification;
using namespace vortia::openfoam;

TEST(ProfileMetrics, IdenticalProfilesHaveZeroError) {
    const std::vector<double> a{1.0, -2.0, 0.5};
    const auto e = compareProfiles(a, a);
    EXPECT_EQ(e.count, 3u);
    EXPECT_DOUBLE_EQ(e.l2, 0.0);
    EXPECT_DOUBLE_EQ(e.linf, 0.0);
    EXPECT_DOUBLE_EQ(e.relL2, 0.0);
    EXPECT_DOUBLE_EQ(e.relLinf, 0.0);
}

TEST(ProfileMetrics, DetectsKnownAbsoluteError) {
    const std::vector<double> ref{0.0, 1.0, 2.0};
    const std::vector<double> cand{0.0, 1.1, 1.9};
    const auto e = compareProfiles(ref, cand);
    EXPECT_NEAR(e.linf, 0.1, 1e-12);
    EXPECT_GT(e.l2, 0.0);
    EXPECT_GT(e.relL2, 0.0);
}

TEST(Poiseuille, WallsHaveZeroVelocityAndPeakAtMidplane) {
    PoiseuilleChannel c;
    EXPECT_NEAR(analyticalU(c, 0.0), 0.0, 1e-15);
    EXPECT_NEAR(analyticalU(c, c.height), 0.0, 1e-15);
    const double umax = analyticalUmax(c);
    EXPECT_GT(umax, 0.0);
    EXPECT_NEAR(analyticalU(c, 0.5 * c.height), umax, 1e-15);
    // Umax = (-dp/dx) H² / (8 ν)
    const double expected = (-pressureGradient(c)) * c.height * c.height / (8.0 * c.nu);
    EXPECT_NEAR(umax, expected, 1e-12);
}

TEST(Poiseuille, CaseSetupIsStructurallyValidAndWritable) {
    const auto setup = makePoiseuilleCase(PoiseuilleChannel{});
    const auto issues = setup.validate();
    for (const auto& issue : issues)
        EXPECT_NE(issue.severity, Severity::Error) << issue.message;
    EXPECT_NEAR(setup.estimateReynolds(), 0.0, 1e-12); // no FixedValue velocity → Re heuristic 0
    Installation i;
    i.distribution = Distribution::Esi;
    i.version = Version::parse("v2606").value();
    const auto caps = capabilitiesFor(i);
    EXPECT_FALSE(CaseWriter::controlDict(setup, caps).empty());
    EXPECT_NE(CaseWriter::controlDict(setup, caps).find("simpleFoam"), std::string::npos);
}

TEST(GhiaCavity, ReferenceTablesMatchPublishedEndpoints) {
    const auto& u = ghiaRe100UCenterline();
    ASSERT_FALSE(u.empty());
    EXPECT_DOUBLE_EQ(u.front().coordinate, 1.0);
    EXPECT_DOUBLE_EQ(u.front().value, 1.0);
    EXPECT_DOUBLE_EQ(u.back().coordinate, 0.0);
    EXPECT_DOUBLE_EQ(u.back().value, 0.0);
    // Mid-cavity sample from Ghia Table I, Re=100.
    bool foundMid = false;
    for (const auto& s : u) {
        if (std::abs(s.coordinate - 0.5) < 1e-9) {
            EXPECT_NEAR(s.value, -0.20581, 1e-9);
            foundMid = true;
        }
    }
    EXPECT_TRUE(foundMid);

    const auto& v = ghiaRe100VCenterline();
    ASSERT_FALSE(v.empty());
    EXPECT_DOUBLE_EQ(v.front().value, 0.0);
    EXPECT_DOUBLE_EQ(v.back().value, 0.0);
    EXPECT_FALSE(std::string(ghiaCitation()).empty());
}

TEST(GhiaCavity, DefaultCaseHasReynoldsOneHundred) {
    const auto setup = makeGhiaCavityCase();
    const auto issues = setup.validate();
    for (const auto& issue : issues)
        EXPECT_NE(issue.severity, Severity::Error) << issue.message;
    EXPECT_NEAR(setup.estimateReynolds(), 100.0, 1e-9);
}
