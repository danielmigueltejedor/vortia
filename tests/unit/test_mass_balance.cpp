#include "vortia/solver/openfoam/MassBalance.hpp"

#include <filesystem>
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
