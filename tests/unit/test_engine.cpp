// vortia-engine's protocol, without OpenFOAM: what a client can do before any run.

#include "Engine.hpp"
#include "PrivateTempDir.hpp"

#include <gtest/gtest.h>

#include <filesystem>
#include <vector>

namespace {

using nlohmann::json;
namespace fs = std::filesystem;

struct EngineTest : ::testing::Test {
    fs::path root = vortia::test::makePrivateTempDir("vortia-engine-test-");
    std::vector<json> events;
    vortia::engine::Engine engine{[this](const json& e) { events.push_back(e); }};

    void TearDown() override { fs::remove_all(root); }

    json ask(json request) {
        request["id"] = 7;
        json reply = engine.handle(request);
        EXPECT_EQ(reply["id"], 7);
        return reply;
    }
};

TEST_F(EngineTest, CreatesAProjectFromTheCavityAndOpensItAgain) {
    const json created = ask({{"cmd", "create"}, {"path", (root / "Cavity").string()}, {"name", "Cavity"}});
    ASSERT_TRUE(created["ok"]) << created.dump();
    EXPECT_EQ(created["result"]["name"], "Cavity");
    EXPECT_EQ(created["result"]["state"], "notCreated");
    EXPECT_EQ(created["result"]["setup"]["solver"], "icoFoam");

    json setup = created["result"]["setup"];
    setup["mesh"]["cells"] = {10, 12, 1};
    ASSERT_TRUE(ask({{"cmd", "save"}, {"setup", setup}})["ok"]);

    const json opened = ask({{"cmd", "open"}, {"path", (root / "Cavity").string()}});
    ASSERT_TRUE(opened["ok"]) << opened.dump();
    EXPECT_EQ(opened["result"]["setup"]["mesh"]["cells"], json({10, 12, 1}));
}

TEST_F(EngineTest, ValidatesWithTheSameRulesAsTheDesktopApp) {
    const json created = ask({{"cmd", "create"}, {"path", (root / "V").string()}, {"name", "V"}});
    json setup = created["result"]["setup"];
    const json fine = ask({{"cmd", "validate"}, {"setup", setup}});
    ASSERT_TRUE(fine["ok"]);
    EXPECT_TRUE(fine["result"]["issues"].empty());
    EXPECT_NEAR(fine["result"]["reynolds"].get<double>(), 100.0, 1e-9);

    setup["time"]["deltaT"] = 0.05;
    const json fast = ask({{"cmd", "validate"}, {"setup", setup}});
    EXPECT_GT(fast["result"]["courant"].get<double>(), 1.0);
    EXPECT_FALSE(fast["result"]["issues"].empty());
}

TEST_F(EngineTest, RefusesWhatItCannotDoWithAReasonRatherThanACrash) {
    EXPECT_EQ(ask({{"cmd", "frob"}})["error"], "unknown command \"frob\"");
    EXPECT_FALSE(ask({{"cmd", "save"}})["ok"]);       // no project
    EXPECT_FALSE(ask({{"cmd", "results"}})["ok"]);    // no project
    EXPECT_FALSE(ask({{"cmd", "validate"}, {"setup", {{"bad", 1}}}})["ok"]);
    EXPECT_FALSE(ask({{"cmd", "open"}, {"path", (root / "missing").string()}})["ok"]);
    EXPECT_TRUE(events.empty());
}

} // namespace
