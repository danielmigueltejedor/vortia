// flowConfiguration must say exactly what the flow setup panel says, so the desktop app and
// vortia-engine start every compressible case from the same settings.

#include "vortia/geometry/SketchTemplates.hpp"
#include "vortia/ui/FlowConfiguration.hpp"
#include "vortia/ui/FlowSetup.hpp"

#include <gtest/gtest.h>

#include <QJsonArray>
#include <QJsonDocument>

namespace {

using namespace vortia;

Sketch2D nozzle() { return makeNozzleProfile({}).value(); }

Sketch2D withArc() {
    Sketch2D s;
    for (const auto p : {SketchPoint2D{0, 0}, {100, 0}, {100, 40}, {0, 30}})
        EXPECT_TRUE(s.addPoint(p));
    EXPECT_TRUE(s.close());
    EXPECT_TRUE(s.setSegmentBulge(2, 0.15));
    return s;
}

void expectSame(const Sketch2D& shape, const QJsonObject& previous, const QJsonObject& defaults = {}) {
    const auto panel = FlowSetupPanel(shape, previous, nullptr, defaults).configuration();
    const auto core = flowConfiguration(shape, previous, defaults);
    EXPECT_EQ(QJsonDocument(core).toJson(), QJsonDocument(panel).toJson());
}

TEST(FlowConfiguration, NewNozzleMatchesThePanel) { expectSame(nozzle(), {}); }

TEST(FlowConfiguration, ArcsAndTheAxisMatchThePanel) { expectSame(withArc(), {}); }

TEST(FlowConfiguration, SavedValuesAndDefaultsMatchThePanel) {
    const auto first = flowConfiguration(nozzle(), {});
    QJsonObject previous = first;
    auto material = previous.value("material").toObject();
    material.insert("turbulence", "inviscid");
    material.insert("Cp", 1234.56789);
    previous.insert("material", material);
    auto solver = previous.value("solver").toObject();
    solver.insert("iterations", 300);
    solver.insert("writeInterval", 900);
    solver.insert("maxCo", 0.9);
    auto convergence = solver.value("convergence").toObject();
    convergence.insert("enabled", false);
    convergence.insert("interval", 700);
    solver.insert("convergence", convergence);
    previous.insert("solver", solver);
    auto boundaries = previous.value("boundaries").toArray();
    auto inlet = boundaries[2].toObject();
    inlet.insert("type", "inlet");
    inlet.insert("name", "  intake  ");
    inlet.insert("pressure", 250000.04);
    boundaries[2] = inlet;
    auto bogus = boundaries[3].toObject();
    bogus.insert("type", "teleporter");
    boundaries[3] = bogus;
    previous.insert("boundaries", boundaries);
    const QJsonObject defaults{{"mesh", QJsonObject{{"sizeMm", 3.14159}, {"wallSizeMm", 9}}}};
    expectSame(nozzle(), previous, defaults);
}

TEST(FlowConfiguration, MeshRegionsMatchThePanel) {
    QJsonObject previous;
    previous.insert("mesh", QJsonObject{
        {"sizeMm", 12},
        {"partitions", QJsonArray{QJsonObject{{"from", QJsonArray{1, 0}}, {"to", QJsonArray{1, 20}}}}},
        {"regions", QJsonArray{QJsonObject{{"id", 1}, {"name", "core"}, {"method", "quads"}},
                               QJsonObject{{"id", 2}, {"sizeMm", 0.123456789}, {"method", "hexes"}}}}});
    expectSame(nozzle(), previous);
}

} // namespace
