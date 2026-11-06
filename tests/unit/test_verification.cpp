#include <gtest/gtest.h>

#include "vortia/verification/Blasius.hpp"
#include "vortia/verification/Conservation.hpp"
#include "vortia/verification/Gci.hpp"
#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"
#include "vortia/verification/ProfileMetrics.hpp"
#include "vortia/verification/ReferenceCsv.hpp"
#include "vortia/verification/SampleFunctions.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "PrivateTempDir.hpp"

#include <cmath>
#include <filesystem>
#include <fstream>
#include <vector>

using namespace vortia;
using namespace vortia::verification;
using namespace vortia::openfoam;

TEST(Blasius, SkinFrictionAndDeltaMatchClassicalCorrelations) {
    BlasiusPlate p;
    const double x = 0.5 * p.length;
    const double rex = reynoldsX(p, x);
    EXPECT_NEAR(rex, p.freestream * x / p.nu, 1e-12);
    EXPECT_NEAR(localSkinFriction(p, x), 0.664 / std::sqrt(rex), 1e-12);
    EXPECT_NEAR(delta99(p, x), 5.0 * x / std::sqrt(rex), 1e-12);
    EXPECT_DOUBLE_EQ(delta99(p, 0.0), 0.0);
    const auto& table = blasiusFPrimeTable();
    ASSERT_GE(table.size(), 3u);
    EXPECT_DOUBLE_EQ(table.front().eta, 0.0);
    EXPECT_DOUBLE_EQ(table.front().fPrime, 0.0);
    EXPECT_NEAR(table.back().fPrime, 1.0, 1e-9);
    EXPECT_FALSE(std::string(blasiusCitation()).empty());
}

TEST(Blasius, CaseSetupIsStructurallyValid) {
    const auto setup = makeBlasiusCase(BlasiusPlate{});
    const auto issues = setup.validate();
    for (const auto& issue : issues)
        EXPECT_NE(issue.severity, Severity::Error) << issue.message;
    EXPECT_GT(setup.estimateReynolds(), 0.0);
    EXPECT_GT(setup.mesh.grading[1], 1.0); // packs toward the plate at y=0
}

TEST(Conservation, BalancedFluxesPass) {
    FluxConservationInput in;
    in.inletFlux = 1.0;
    in.outletFlux = 1.0;
    in.relativeTolerance = 1e-3;
    const auto r = assessVolumeFluxBalance(in);
    ASSERT_TRUE(r.ok) << r.message;
    EXPECT_NEAR(r.relativeImbalance, 0.0, 1e-15);
}

TEST(Conservation, ImbalancedFluxesFail) {
    FluxConservationInput in;
    in.inletFlux = 1.0;
    in.outletFlux = 0.9;
    in.relativeTolerance = 1e-3;
    const auto r = assessVolumeFluxBalance(in);
    EXPECT_FALSE(r.ok);
    EXPECT_NEAR(r.relativeImbalance, 0.1, 1e-12);
}

TEST(Conservation, ToleranceBoundaryIsInclusive) {
    FluxConservationInput in;
    in.inletFlux = 100.0;
    in.outletFlux = 99.0;
    in.relativeTolerance = 0.01; // |100-99|/100 == 0.01 exactly
    const auto r = assessVolumeFluxBalance(in);
    ASSERT_TRUE(r.ok) << r.message;
    EXPECT_DOUBLE_EQ(r.relativeImbalance, 0.01);
}

TEST(Conservation, TinyOutletAgainstZeroInletFails) {
    FluxConservationInput in;
    in.inletFlux = 0.0;
    in.outletFlux = 1e-34;
    in.relativeTolerance = 1e-3;
    const auto r = assessVolumeFluxBalance(in);
    EXPECT_FALSE(r.ok);
    EXPECT_NEAR(r.relativeImbalance, 1.0, 1e-12);
}

TEST(Conservation, RejectsNegativeSignedMonitors) {
    FluxConservationInput in;
    in.inletFlux = -1.0;
    in.outletFlux = 1.0;
    in.relativeTolerance = 1e-3;
    const auto r = assessVolumeFluxBalance(in);
    EXPECT_FALSE(r.ok);
    EXPECT_NE(r.message.find("non-negative"), std::string::npos);
}

TEST(Conservation, PoiseuilleAnalyticFluxSelfConsistent) {
    PoiseuilleChannel c;
    const double q = analyticalFlowPerDepth(c);
    FluxConservationInput in;
    in.inletFlux = q;
    in.outletFlux = q;
    in.relativeTolerance = 1e-12;
    const auto r = assessVolumeFluxBalance(in);
    ASSERT_TRUE(r.ok) << r.message;
}

TEST(Gci, ExactSecondOrderMonotonicRefinement) {
    // Manufactured: f(h) = f* + C h^2 with f*=1, C=1, h={0.25,0.5,1}.
    GciInput in;
    in.f1 = 1.0 + 0.25 * 0.25;
    in.f2 = 1.0 + 0.5 * 0.5;
    in.f3 = 1.0 + 1.0 * 1.0;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_NEAR(g.observedOrder, 2.0, 1e-9);
    EXPECT_NEAR(g.extrapolated, 1.0, 1e-9);
    EXPECT_GT(g.gci12, 0.0);
    // With equal r, relative GCIs give asym ≈ |f1|/|f2| (not an independent check).
    EXPECT_NEAR(g.asymptoticRatio, std::abs(in.f1) / std::abs(in.f2), 1e-6);
}

TEST(Gci, UnequalRefinementRecoversSecondOrder) {
    // f(h)=1+h² at h={1,2,6} ⇒ r21=2, r32=3.
    GciInput in;
    in.f1 = 1.0 + 1.0;
    in.f2 = 1.0 + 4.0;
    in.f3 = 1.0 + 36.0;
    in.r21 = 2.0;
    in.r32 = 3.0;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_NEAR(g.observedOrder, 2.0, 1e-6);
    EXPECT_NEAR(g.extrapolated, 1.0, 1e-6);
}

TEST(Gci, UnequalRefinementWhenCoarseRatioIsSmaller) {
    // f(h)=1+h² at h={1,2,2.2} ⇒ r21=2, r32=1.1 (seed |ε32/ε21|<1).
    GciInput in;
    in.f1 = 1.0 + 1.0;
    in.f2 = 1.0 + 4.0;
    in.f3 = 1.0 + 2.2 * 2.2;
    in.r21 = 2.0;
    in.r32 = 1.1;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_NEAR(g.observedOrder, 2.0, 1e-5);
    EXPECT_NEAR(g.extrapolated, 1.0, 1e-5);
}

TEST(Gci, RejectsOscillatorySequence) {
    GciInput in;
    in.f1 = 1.0;
    in.f2 = 2.0;
    in.f3 = 0.0;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
    EXPECT_NE(g.message.find("oscillatory"), std::string::npos);
}

TEST(Gci, RejectsTinyOscillatoryWithoutProductUnderflow) {
    GciInput in;
    in.f1 = 1e-170;
    in.f2 = 2e-170;
    in.f3 = -2e-170;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
    EXPECT_NE(g.message.find("oscillatory"), std::string::npos);
}

TEST(Gci, SmallMagnitudeSecondOrderStillComputes) {
    GciInput in;
    in.f1 = 1e-16 + 1e-16 * 0.25 * 0.25;
    in.f2 = 1e-16 + 1e-16 * 0.5 * 0.5;
    in.f3 = 1e-16 + 1e-16 * 1.0;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_NEAR(g.observedOrder, 2.0, 1e-6);
    EXPECT_NEAR(g.extrapolated, 1e-16, 1e-20);
}

TEST(Gci, RejectsInvalidRefinementRatio) {
    GciInput in;
    in.f1 = 1.0;
    in.f2 = 1.1;
    in.f3 = 1.3;
    in.r21 = 1.0;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
}

TEST(Gci, IdenticalMeshesReportZeroUncertainty) {
    GciInput in;
    in.f1 = in.f2 = in.f3 = 42.0;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_DOUBLE_EQ(g.gci12, 0.0);
    EXPECT_DOUBLE_EQ(g.extrapolated, 42.0);
}

TEST(Gci, SignedZerosCountAsIdentical) {
    GciInput in;
    in.f1 = 0.0;
    in.f2 = -0.0;
    in.f3 = 0.0;
    const auto g = computeGci(in);
    ASSERT_TRUE(g.ok) << g.message;
    EXPECT_DOUBLE_EQ(g.gci12, 0.0);
}

TEST(Gci, RejectsEqualRatioDivergentSequence) {
    // |ε32/ε21| < 1 with equal r ⇒ negative observed order / diverging differences.
    GciInput in;
    in.f1 = 1.0;
    in.f2 = 2.0;
    in.f3 = 2.5;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
    EXPECT_NE(g.message.find("equal-ratio"), std::string::npos);
}

TEST(Gci, RejectsEqualRatioErrorsGrowingTowardFineMesh) {
    GciInput in;
    in.f1 = 100.02;
    in.f2 = 100.01;
    in.f3 = 100.005;
    in.r21 = 2.0;
    in.r32 = 2.0;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
}

TEST(Gci, RejectsUnequalRatioWithoutPositiveOrder) {
    // |ε32/ε21|=0.1 < ln(1.3)/ln(2) ≈ 0.379 ⇒ no positive-order Richardson fit.
    GciInput in;
    in.f1 = 1.0;
    in.f2 = 2.0;
    in.f3 = 2.1;
    in.r21 = 2.0;
    in.r32 = 1.3;
    const auto g = computeGci(in);
    EXPECT_FALSE(g.ok);
    EXPECT_NE(g.message.find("unequal-ratio"), std::string::npos);
}

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

TEST(ReferenceCsv, InterpolateRecoversLinearField) {
    const std::vector<Sample1D> sorted{{0.0, 0.0}, {1.0, 2.0}, {2.0, 4.0}};
    const auto ys = interpolateAt(sorted, {0.5, 1.5});
    ASSERT_TRUE(ys) << ys.error().describe();
    EXPECT_NEAR((*ys)[0], 1.0, 1e-12);
    EXPECT_NEAR((*ys)[1], 3.0, 1e-12);
}

TEST(ReferenceCsv, LoadsGhiaTableFromBenchmarkTree) {
    const auto path = std::filesystem::path(VORTIA_BENCHMARK_ROOT)
                      / "lid-driven-cavity-ghia-re100"
                      / "reference"
                      / "ghia_1982_re100_u_centerline.csv";
    const auto rows = loadXyCsv(path);
    ASSERT_TRUE(rows) << rows.error().describe();
    EXPECT_GE(rows->size(), 10u);
    bool foundMid = false;
    for (const auto& s : *rows) {
        if (std::abs(s.x - 0.5) < 1e-9) {
            EXPECT_NEAR(s.y, -0.20581, 1e-9);
            foundMid = true;
        }
    }
    EXPECT_TRUE(foundMid);
}

TEST(BenchmarkCaseWrite, PoiseuilleMaterializesMarkerAndSampleDict) {
    const auto dir = vortia::test::makePrivateTempDir("vortia-bench-poiseuille-");
    const auto setup = makePoiseuilleCase(PoiseuilleChannel{});
    Installation i;
    i.distribution = Distribution::Esi;
    i.version = Version::parse("v2606").value();
    WriteOptions opts;
    opts.overwrite = true;
    const auto written = CaseWriter::write(dir, setup, capabilitiesFor(i), opts);
    ASSERT_TRUE(written) << written.error().describe();
    EXPECT_TRUE(std::filesystem::exists(dir / ".vortia-case"));
    const auto sample = poiseuilleSampleDict(PoiseuilleChannel{});
    {
        std::ofstream out(dir / "system" / "sampleU");
        ASSERT_TRUE(static_cast<bool>(out));
        out << sample;
    }
    EXPECT_TRUE(std::filesystem::exists(dir / "system" / "sampleU"));
    std::filesystem::remove_all(dir);
}
