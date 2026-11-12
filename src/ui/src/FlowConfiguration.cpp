#include "vortia/ui/FlowConfiguration.hpp"

#include "vortia/ui/BoundaryMapping.hpp"

#include <QJsonArray>
#include <QJsonValue>
#include <QString>

#include <algorithm>
#include <cmath>
#include <initializer_list>

namespace vortia {
namespace {

/// What a QDoubleSpinBox stores: clamped to its range, rounded to its decimals.
double spin(double value, double low, double high, int decimals) {
    if (!std::isfinite(value)) value = low;
    value = std::clamp(value, low, high);
    const double scale = std::pow(10.0, decimals);
    return std::clamp(std::round(value * scale) / scale, low, high);
}

struct Field { const char* section; const char* key; double fallback, low, high; int decimals; };

// In the panel's order. Changing a default here changes it for both editions.
constexpr Field kFields[] = {
    {"mesh", "sizeMm", 20, 0.001, 1e7, 3},
    {"mesh", "wallSizeMm", 2, 0.001, 1e7, 3},
    {"mesh", "wedgeAngle", 5, 0.1, 5, 2},
    {"material", "molecularWeight", 28.966, 0.01, 10000, 5},
    {"material", "Cp", 1005, 0.01, 1e7, 3},
    {"material", "mu", 1.8e-5, 1e-12, 100, 10},
    {"material", "conductivity", 0.025, 1e-9, 10000, 7},
    {"initial", "pressure", 101325, 0.001, 1e9, 1},
    {"initial", "temperature", 300, 0.001, 100000, 2},
    {"solver", "iterations", 50000, 1, 200000, 0},
    {"solver", "writeInterval", 100, 1, 200000, 0},
    {"solver", "maxCo", 0.1, 0.001, 0.5, 6},
    {"solver", "logInterval", 10, 1, 1000, 0},
    {"solver", "processes", 0, 0, 8, 0},
};

constexpr Field kCriteria[] = {
    {"", "interval", 50, 1, 10000, 0},
    {"", "minimumIterations", 500, 1, 200000, 0},
    {"", "stableSamples", 5, 2, 100, 0},
    {"", "massTolerance", .001, .00000001, .01, 8},
    {"", "fieldTolerance", .001, .00000001, .01, 8},
    {"", "equationTolerance", 1e-5, 1e-8, 1e-3, 8},
    {"", "absoluteMassTolerance", 1e-9, 1e-15, 1e-6, 15},
};

bool onAxis(const Sketch2D& shape, std::size_t index) {
    const auto& points = shape.points();
    const auto a = points[index], b = points[(index + 1) % points.size()];
    return a.yMm == 0 && b.yMm == 0 && shape.segmentBulge(index) == 0;
}

/// The mesh regions block as MeshRegionsWidget keeps and returns it.
QJsonObject meshSettings(const QJsonObject& previousMesh, const QJsonObject& globals) {
    QJsonObject settings = previousMesh;
    for (auto it = globals.begin(); it != globals.end(); ++it) settings.insert(it.key(), it.value());
    const auto cuts = previousMesh.value("partitions").toArray();
    QJsonArray regions = previousMesh.value("regions").toArray();
    for (qsizetype i = 0; i < regions.size(); ++i) {
        auto region = regions[i].toObject();
        // The region editor's name field holds 120 characters; its size falls back to the global
        // size as it was saved, before this setup's own global size is applied.
        region.insert("name", region.value("name").toString().left(120));
        region.insert("sizeMm", spin(region.value("sizeMm").toDouble(previousMesh.value("sizeMm").toDouble(20)), .001, 1e7, 6));
        const auto method = region.value("method").toString();
        region.insert("method", method == "quads" ? "quads" : "triangles");
        regions[i] = region;
    }
    if (settings.contains("engine") || !cuts.isEmpty() || !regions.isEmpty()) {
        settings.insert("engine", "occ-regions-v1");
        settings.insert("partitions", cuts);
        QJsonArray controls;
        for (const auto& value : regions) {
            const auto r = value.toObject();
            QJsonObject control;
            for (const auto* key : {"id", "name", "seed", "sizeMm", "method"})
                if (r.contains(key)) control.insert(key, r.value(key));
            controls.append(control);
        }
        settings.insert("regions", controls);
    }
    return settings;
}

} // namespace

QJsonObject flowConfiguration(const Sketch2D& shape, const QJsonObject& previous, const QJsonObject& defaults) {
    const auto working = restoreBoundaryGeometry(shape, previous);
    QJsonObject result;
    for (const auto& f : kFields) {
        const double fallback = defaults.value(f.section).toObject().value(f.key).toDouble(f.fallback);
        const double value = previous.value(f.section).toObject().value(f.key).toDouble(fallback);
        auto section = result.value(f.section).toObject();
        section.insert(f.key, spin(value, f.low, f.high, f.decimals));
        result.insert(f.section, section);
    }

    // The panel's rules between fields: the wall size never exceeds the global size, nor the
    // save interval the iteration limit.
    auto mesh = result.value("mesh").toObject();
    mesh.insert("wallSizeMm", std::min(mesh.value("wallSizeMm").toDouble(), mesh.value("sizeMm").toDouble()));
    auto solver = result.value("solver").toObject();
    solver.insert("writeInterval", std::min(solver.value("writeInterval").toDouble(), solver.value("iterations").toDouble()));

    const auto convergence = previous.value("solver").toObject().value("convergence").toObject();
    QJsonObject criteria{{"enabled", convergence.value("enabled").toBool(true)}};
    for (const auto& c : kCriteria)
        criteria.insert(c.key, spin(convergence.value(c.key).toDouble(c.fallback), c.low, c.high, c.decimals));
    if (criteria.value("minimumIterations").toDouble() < criteria.value("interval").toDouble())
        criteria.insert("minimumIterations", criteria.value("interval").toDouble());
    solver.insert("convergence", criteria);
    result.insert("solver", solver);

    result.insert("mesh", meshSettings(previous.value("mesh").toObject(), mesh));

    auto material = result.value("material").toObject();
    const auto model = previous.value("material").toObject().value("turbulence").toString("SpalartAllmaras");
    material.insert("turbulence", model == "laminar" || model == "inviscid" ? model : QString("SpalartAllmaras"));
    result.insert("material", material);

    const auto old = working.value("boundaries").toArray();
    const auto rows = static_cast<qsizetype>(shape.segmentCount());
    QJsonArray boundaries;
    for (qsizetype row = 0; row < rows; ++row) {
        const auto index = static_cast<std::size_t>(row);
        const bool axis = onAxis(shape, index);
        const auto previousRow = old.size() == rows ? old[row].toObject() : QJsonObject{};
        const auto type = axis ? QString("axis") : previousRow.value("type").toString("wall");
        static const QStringList types{"wall", "axis", "inlet", "outlet", "reservoir", "unassigned"};
        QJsonObject bc;
        bc.insert("name", previousRow.value("name").toString(axis ? QString("axis") : QString("edge_%1").arg(row + 1)).trimmed());
        bc.insert("type", types.contains(type) ? type : QString("wall"));
        bc.insert("pressure", spin(previousRow.value("pressure").toDouble(101325), 0.001, 1e9, 1));
        bc.insert("temperature", spin(previousRow.value("temperature").toDouble(300), 0.001, 1e9, 3));
        bc.insert("viscosityRatio", spin(previousRow.value("viscosityRatio").toDouble(10), 0.001, 100000, 3));
        boundaries.append(bc);
    }
    result.insert("boundaries", normalizedBoundaryNames(boundaries));
    result.insert("boundaryGeometry", boundaryGeometry(shape));
    for (const auto* key : {"boundaryVersions", "unmappedBoundaries"})
        if (working.contains(key)) result.insert(key, working.value(key));
    return result;
}

} // namespace vortia
