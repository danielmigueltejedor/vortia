#include "vortia/geometry/SketchFile.hpp"

#include <nlohmann/json.hpp>

#include <cmath>
#include <cstddef>

namespace vortia {

using nlohmann::json;

namespace {

/// Covers the editor's coordinate range and the longest nozzle; larger values cannot be drawn.
constexpr double kCoordinateLimit = 2000000.0;
constexpr std::size_t kVertexLimit = 10000;

Error invalid(std::string message) {
    return Error::make(ErrorCode::InvalidData, std::move(message), "sketch");
}

} // namespace

std::string sketchToJson(const SketchDocument& document) {
    const auto& sketch = document.sketch;
    json root{{"schemaVersion", sketch.hasArcs() ? 2 : 1}, {"units", "mm"}, {"closed", sketch.closed()}};
    json vertices = json::array();
    for (const auto& p : sketch.points())
        vertices.push_back({p.xMm, p.yMm});
    root["vertices"] = std::move(vertices);
    if (sketch.hasArcs()) {
        json bulges = json::array();
        for (std::size_t i = 0; i < sketch.segmentCount(); ++i)
            bulges.push_back(sketch.segmentBulge(i));
        root["segmentBulges"] = std::move(bulges);
    }
    const auto& n = document.nozzle;
    root["nozzleDimensions"] = {{"inletDiameterMm", n.inletDiameterMm},
                                {"throatDiameterMm", n.throatDiameterMm},
                                {"outletDiameterMm", n.outletDiameterMm},
                                {"convergingLengthMm", n.convergingLengthMm},
                                {"divergingLengthMm", n.divergingLengthMm}};
    return root.dump(4);
}

Expected<SketchDocument> sketchFromJson(std::string_view text) {
    const json root = json::parse(text, nullptr, false);
    if (root.is_discarded() || !root.is_object())
        return invalid("not a JSON object");
    const auto version = root.find("schemaVersion");
    if (version == root.end() || !version->is_number())
        return invalid("missing schemaVersion");
    const double schema = version->get<double>();
    if (schema != 1.0 && schema != 2.0)
        return invalid("unsupported schemaVersion");
    if (root.value("units", std::string{}) != "mm")
        return invalid("units must be mm");
    const auto vertices = root.find("vertices");
    const auto closed = root.find("closed");
    if (vertices == root.end() || !vertices->is_array() || closed == root.end() || !closed->is_boolean())
        return invalid("missing vertices or closed");
    if (vertices->size() > kVertexLimit)
        return invalid("too many vertices");

    SketchDocument document;
    for (const auto& row : *vertices) {
        if (!row.is_array() || row.size() != 2 || !row[0].is_number() || !row[1].is_number())
            return invalid("a vertex is not two numbers");
        const double x = row[0].get<double>();
        const double y = row[1].get<double>();
        if (std::abs(x) > kCoordinateLimit || std::abs(y) > kCoordinateLimit || !document.sketch.addPoint({x, y}))
            return invalid("a vertex is out of range or repeats the previous one");
    }
    if (closed->get<bool>() && !document.sketch.close())
        return invalid("the contour cannot be closed");

    const auto bulges = root.find("segmentBulges");
    if (schema == 2.0) {
        if (bulges == root.end() || !bulges->is_array() || bulges->size() != document.sketch.segmentCount())
            return invalid("segmentBulges does not match the segments");
        for (std::size_t i = 0; i < bulges->size(); ++i)
            if (!(*bulges)[i].is_number() || !document.sketch.setSegmentBulge(i, (*bulges)[i].get<double>()))
                return invalid("an arc is not valid");
    } else if (bulges != root.end()) {
        return invalid("schema 1 has no arcs");
    }

    if (const auto nozzle = root.find("nozzleDimensions"); nozzle != root.end()) {
        if (!nozzle->is_object())
            return invalid("nozzleDimensions is not an object");
        double values[5];
        const char* keys[5] = {"inletDiameterMm", "throatDiameterMm", "outletDiameterMm",
                               "convergingLengthMm", "divergingLengthMm"};
        for (int i = 0; i < 5; ++i) {
            const auto value = nozzle->find(keys[i]);
            if (value == nozzle->end() || !value->is_number())
                return invalid(std::string("nozzleDimensions.") + keys[i] + " is not a number");
            values[i] = value->get<double>();
        }
        const NozzleDimensions loaded{values[0], values[1], values[2], values[3], values[4]};
        if (!makeNozzleProfile(loaded))
            return invalid("the nozzle dimensions do not describe a nozzle");
        document.nozzle = loaded;
    }
    return document;
}

} // namespace vortia
