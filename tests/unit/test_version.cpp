#include <gtest/gtest.h>

#include "vortia/core/Version.hpp"

using namespace vortia;

TEST(Version, ParsesEsiStyle) {
    auto v = Version::parse("v2606");
    ASSERT_TRUE(v.hasValue());
    EXPECT_EQ(v->major, 2606);
    EXPECT_EQ(v->minor, 0);
    EXPECT_EQ(v->raw, "v2606");
}

TEST(Version, ParsesFoundationStyle) {
    auto v = Version::parse("14");
    ASSERT_TRUE(v.hasValue());
    EXPECT_EQ(v->major, 14);
}

TEST(Version, ParsesDotted) {
    auto v = Version::parse("9.4.2");
    ASSERT_TRUE(v.hasValue());
    EXPECT_EQ(v->major, 9);
    EXPECT_EQ(v->minor, 4);
    EXPECT_EQ(v->patch, 2);
}

TEST(Version, ToleratesWhitespaceAndSuffix) {
    auto v = Version::parse("  v2412-patch1 ");
    ASSERT_TRUE(v.hasValue());
    EXPECT_EQ(v->major, 2412);
}

TEST(Version, RejectsGarbage) {
    EXPECT_FALSE(Version::parse("").hasValue());
    EXPECT_FALSE(Version::parse("abc").hasValue());
    EXPECT_FALSE(Version::parse(".1").hasValue());
}
