#include <gtest/gtest.h>

#include "vortia/solver/openfoam/Installation.hpp"

using namespace vortia;
using namespace vortia::openfoam;

namespace {

Installation make(Distribution d, std::string version) {
    Installation i;
    i.distribution = d;
    i.version = Version::parse(version).value();
    i.rootDir = "/opt/fake";
    return i;
}

} // namespace

TEST(Installation, Fingerprint) {
    const auto i = make(Distribution::Esi, "v2606");
    EXPECT_EQ(i.fingerprint(), "esi/v2606@/opt/fake");
}

TEST(Capabilities, EsiKeepsClassicSolvers) {
    const auto caps = capabilitiesFor(make(Distribution::Esi, "v2606"));
    EXPECT_TRUE(caps.hasIcoFoam);
    EXPECT_TRUE(caps.hasSimpleFoam);
    EXPECT_EQ(caps.laminarTransientSolver, "icoFoam");
    EXPECT_EQ(caps.laminarSteadySolver, "simpleFoam");
    EXPECT_EQ(caps.transportDict, "transportProperties");
    EXPECT_TRUE(caps.needsVerification()); // not verified on a system yet
}

TEST(Capabilities, Foundation10PlusUsesFoamRun) {
    const auto caps = capabilitiesFor(make(Distribution::Foundation, "11"));
    EXPECT_FALSE(caps.hasIcoFoam);
    EXPECT_TRUE(caps.hasFoamRun);
    EXPECT_EQ(caps.laminarTransientSolver, "foamRun");
    ASSERT_EQ(caps.laminarTransientArgs.size(), 2u);
    EXPECT_EQ(caps.laminarTransientArgs[1], "incompressibleFluid");
}

TEST(Capabilities, Foundation12RenamesTransportDict) {
    const auto caps = capabilitiesFor(make(Distribution::Foundation, "12"));
    EXPECT_EQ(caps.transportDict, "physicalProperties");
}

TEST(Capabilities, Foundation9KeepsClassicSolvers) {
    const auto caps = capabilitiesFor(make(Distribution::Foundation, "9"));
    EXPECT_TRUE(caps.hasIcoFoam);
    EXPECT_EQ(caps.transportDict, "transportProperties");
}
