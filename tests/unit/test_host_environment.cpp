#include "vortia/core/HostEnvironment.hpp"

#include <gtest/gtest.h>

#include <map>

namespace {

struct FakeEnvironment {
    std::map<std::string, std::string> values;

    vortia::EnvironmentAccess access() {
        return {
            [this](const std::string& name) -> std::optional<std::string> {
                const auto it = values.find(name);
                return it == values.end() ? std::nullopt : std::optional<std::string>{it->second};
            },
            [this](const std::string& name, const std::string& value) { values[name] = value; },
            [this](const std::string& name) { values.erase(name); },
        };
    }
};

} // namespace

TEST(HostEnvironment, LeavesEnvironmentAloneWithoutLauncher) {
    FakeEnvironment env;
    env.values = {{"LD_LIBRARY_PATH", "/opt/openfoam/lib"}, {"VORTIA_HOST_LD_LIBRARY_PATH", "/stale"}};
    EXPECT_FALSE(vortia::restoreHostVariable(env.access(), "LD_LIBRARY_PATH"));
    EXPECT_EQ(env.values.at("LD_LIBRARY_PATH"), "/opt/openfoam/lib");
}

TEST(HostEnvironment, RestoresTheValueTheUserHadBeforeTheLauncher) {
    FakeEnvironment env;
    env.values = {{"VORTIA_LAUNCHER_ENV", "1"},
                  {"LD_LIBRARY_PATH", "/tmp/.mount_Vortia/usr/lib:/opt/openfoam/lib"},
                  {"VORTIA_HOST_LD_LIBRARY_PATH", "/opt/openfoam/lib"}};
    EXPECT_TRUE(vortia::restoreHostVariable(env.access(), "LD_LIBRARY_PATH"));
    EXPECT_EQ(env.values.at("LD_LIBRARY_PATH"), "/opt/openfoam/lib");
}

TEST(HostEnvironment, UnsetsVariablesTheUserNeverHad) {
    FakeEnvironment env;
    env.values = {{"VORTIA_LAUNCHER_ENV", "1"}, {"QT_PLUGIN_PATH", "/tmp/.mount_Vortia/usr/plugins"}};
    EXPECT_TRUE(vortia::restoreHostVariable(env.access(), "QT_PLUGIN_PATH"));
    EXPECT_EQ(env.values.count("QT_PLUGIN_PATH"), 0U);
}

TEST(HostEnvironment, KeepsAnOriginallyEmptyValueDistinctFromUnset) {
    FakeEnvironment env;
    env.values = {{"VORTIA_LAUNCHER_ENV", "1"},
                  {"LD_LIBRARY_PATH", "/tmp/.mount_Vortia/usr/lib"},
                  {"VORTIA_HOST_LD_LIBRARY_PATH", ""}};
    EXPECT_TRUE(vortia::restoreHostVariable(env.access(), "LD_LIBRARY_PATH"));
    ASSERT_EQ(env.values.count("LD_LIBRARY_PATH"), 1U);
    EXPECT_EQ(env.values.at("LD_LIBRARY_PATH"), "");
}

TEST(HostEnvironment, ProcessEnvironmentRoundTrips) {
    const auto env = vortia::processEnvironment();
    env.set("VORTIA_TEST_HOST_ENV", "value");
    EXPECT_EQ(env.get("VORTIA_TEST_HOST_ENV"), std::optional<std::string>{"value"});
    env.unset("VORTIA_TEST_HOST_ENV");
    EXPECT_EQ(env.get("VORTIA_TEST_HOST_ENV"), std::nullopt);
}
