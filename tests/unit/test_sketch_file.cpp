#include "vortia/geometry/SketchFile.hpp"

#include <gtest/gtest.h>

using namespace vortia;

namespace {

Sketch2D square(bool arc) {
    Sketch2D s;
    for (const auto p : {SketchPoint2D{0, 0}, {10, 0}, {10, 10}, {0, 10}})
        EXPECT_TRUE(s.addPoint(p));
    EXPECT_TRUE(s.close());
    if (arc)
        EXPECT_TRUE(s.setSegmentBulge(2, 0.2));
    return s;
}

} // namespace

TEST(SketchFile, StraightContourRoundTripsAsSchemaOne) {
    const std::string text = sketchToJson({square(false), {}});
    EXPECT_NE(text.find("\"schemaVersion\": 1"), std::string::npos);
    EXPECT_EQ(text.find("segmentBulges"), std::string::npos);
    const auto read = sketchFromJson(text);
    ASSERT_TRUE(read) << read.error().describe();
    EXPECT_TRUE(read->sketch.closed());
    EXPECT_EQ(read->sketch.points().size(), 4u);
    EXPECT_FALSE(read->sketch.hasArcs());
}

TEST(SketchFile, ArcsAndNozzleDimensionsSurvive) {
    NozzleDimensions nozzle;
    nozzle.throatDiameterMm = 40;
    const auto read = sketchFromJson(sketchToJson({square(true), nozzle}));
    ASSERT_TRUE(read) << read.error().describe();
    EXPECT_NEAR(read->sketch.segmentBulge(2), 0.2, 1e-12);
    EXPECT_EQ(read->nozzle.throatDiameterMm, 40);
}

TEST(SketchFile, ReadsWhatTheDesktopEditorWrote) {
    // The Qt editor's own output: integers without a decimal point, keys in its order.
    const auto read = sketchFromJson(R"({"closed": true, "schemaVersion": 1, "units": "mm",
        "vertices": [[0, 0], [10, 0], [10, 10]],
        "nozzleDimensions": {"convergingLengthMm": 158, "divergingLengthMm": 420,
            "inletDiameterMm": 100, "outletDiameterMm": 120, "throatDiameterMm": 52}})");
    ASSERT_TRUE(read) << read.error().describe();
    EXPECT_EQ(read->sketch.segmentCount(), 3u);
}

TEST(SketchFile, RefusesAnythingItDidNotWrite) {
    EXPECT_FALSE(sketchFromJson("[]"));
    EXPECT_FALSE(sketchFromJson(R"({"schemaVersion": 3, "units": "mm", "closed": false, "vertices": []})"));
    EXPECT_FALSE(sketchFromJson(R"({"schemaVersion": 1, "units": "in", "closed": false, "vertices": []})"));
    EXPECT_FALSE(sketchFromJson(R"({"schemaVersion": 1, "units": "mm", "closed": false, "vertices": [[0, 0], [0, 0]]})"));
    EXPECT_FALSE(sketchFromJson(R"({"schemaVersion": 1, "units": "mm", "closed": false, "vertices": [[0, 0], [1, 1]], "segmentBulges": [0]})"));
    EXPECT_FALSE(sketchFromJson(R"({"schemaVersion": 1, "units": "mm", "closed": false, "vertices": [[0, 0]],
        "nozzleDimensions": {"inletDiameterMm": 10, "throatDiameterMm": 52, "outletDiameterMm": 120,
        "convergingLengthMm": 1, "divergingLengthMm": 1}})"));
}
