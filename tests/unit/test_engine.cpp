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

TEST_F(EngineTest, ANozzleBecomesASketchAndAFlowSetup) {
    ASSERT_TRUE(ask({{"cmd", "create"}, {"path", (root / "N").string()}, {"name", "N"}})["ok"]);
    EXPECT_EQ(ask({{"cmd", "sketch"}})["result"]["sketch"], nullptr);
    EXPECT_FALSE(ask({{"cmd", "flow"}})["ok"]); // no contour yet

    const json dimensions{{"inletDiameterMm", 100}, {"throatDiameterMm", 52}, {"outletDiameterMm", 120},
                          {"convergingLengthMm", 158}, {"divergingLengthMm", 420}};
    const json made = ask({{"cmd", "nozzle"}, {"dimensions", dimensions}});
    ASSERT_TRUE(made["ok"]) << made.dump();
    const auto& sketch = made["result"]["sketch"];
    EXPECT_TRUE(sketch["closed"].get<bool>());
    EXPECT_TRUE(sketch["segments"][0]["axis"].get<bool>()); // the bottom edge lies on Y=0

    const json flow = ask({{"cmd", "flow"}});
    ASSERT_TRUE(flow["ok"]) << flow.dump();
    auto config = flow["result"]["flow"];
    EXPECT_EQ(config["boundaries"].size(), sketch["segments"].size());
    EXPECT_EQ(config["boundaries"][0]["type"], "axis");
    EXPECT_EQ(config["material"]["turbulence"], "SpalartAllmaras");

    config["boundaries"][1]["type"] = "outlet";
    config["solver"]["iterations"] = 1234;
    const json saved = ask({{"cmd", "saveFlow"}, {"flow", config}});
    ASSERT_TRUE(saved["ok"]) << saved.dump();
    const json again = ask({{"cmd", "flow"}});
    EXPECT_EQ(again["result"]["flow"]["boundaries"][1]["type"], "outlet");
    EXPECT_EQ(again["result"]["flow"]["solver"]["iterations"], 1234);

    const json narrow = ask({{"cmd", "nozzle"}, {"dimensions", {{"inletDiameterMm", 10}, {"throatDiameterMm", 52},
        {"outletDiameterMm", 120}, {"convergingLengthMm", 1}, {"divergingLengthMm", 1}}}});
    EXPECT_FALSE(narrow["ok"]);
}

TEST_F(EngineTest, ASketchIsSavedOnlyIfItIsOne) {
    ASSERT_TRUE(ask({{"cmd", "create"}, {"path", (root / "S").string()}, {"name", "S"}})["ok"]);
    const json square{{"schemaVersion", 1}, {"units", "mm"}, {"closed", true},
                      {"vertices", {{0, 0}, {100, 0}, {100, 50}, {0, 50}}}};
    const json saved = ask({{"cmd", "saveSketch"}, {"sketch", square}});
    ASSERT_TRUE(saved["ok"]) << saved.dump();
    EXPECT_EQ(saved["result"]["sketch"]["segments"].size(), 4u);
    json broken = square;
    broken["units"] = "in";
    EXPECT_FALSE(ask({{"cmd", "saveSketch"}, {"sketch", broken}})["ok"]);
    EXPECT_EQ(ask({{"cmd", "sketch"}})["result"]["sketch"]["points"].size(), 4u);
}

} // namespace
