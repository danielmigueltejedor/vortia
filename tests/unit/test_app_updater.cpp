#include "vortia/ui/AppUpdater.hpp"

#include <gtest/gtest.h>

using vortia::compareAppVersions;

TEST(AppUpdater, CompareVersions) {
    EXPECT_LT(compareAppVersions("0.1.0", "0.2.0"), 0);
    EXPECT_GT(compareAppVersions("v0.2.0", "0.1.9"), 0);
    EXPECT_EQ(compareAppVersions("0.1.0", "v0.1.0"), 0);
    EXPECT_LT(compareAppVersions("0.1.0-beta", "0.1.1"), 0);
}
