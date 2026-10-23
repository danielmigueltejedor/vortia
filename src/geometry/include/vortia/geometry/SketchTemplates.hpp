#pragma once

#include "vortia/geometry/Sketch2D.hpp"

#include <cmath>
#include <optional>

namespace vortia {

struct NozzleDimensions {
    double inletDiameterMm{100.0};
    double throatDiameterMm{52.0};
    double outletDiameterMm{120.0};
    double convergingLengthMm{158.0};
    double divergingLengthMm{420.0};
};

/// Creates a CLOSED meridional fluid profile above the rotation axis Y=0.
/// All dimensions are millimetres; this is geometry ONLY, not a CFD wedge.
inline std::optional<Sketch2D> makeNozzleProfile(const NozzleDimensions& d) {
    const double values[] = {d.inletDiameterMm, d.throatDiameterMm,
                             d.outletDiameterMm, d.convergingLengthMm,
                             d.divergingLengthMm};
    for (double v : values)
        if (!std::isfinite(v) || v <= 0.0 || v > 1e6) return std::nullopt;
    if (d.throatDiameterMm > d.inletDiameterMm ||
        d.throatDiameterMm > d.outletDiameterMm)
        return std::nullopt;

    constexpr int segments = 20;
    Sketch2D result;
    const double total = d.convergingLengthMm + d.divergingLengthMm;
    if (!result.addPoint({0, 0}) || !result.addPoint({total, 0}) ||
        !result.addPoint({total, d.outletDiameterMm / 2.0}))
        return std::nullopt;
    // Smoothstep has zero slope at inlet/throat/outlet; segments are
    // a polygonal approximation of the smooth analytical nozzle wall.
    auto smooth = [](double t) { return t * t * (3.0 - 2.0 * t); };
    for (int i = segments - 1; i >= 0; --i) {
        const double t = static_cast<double>(i) / segments;
        const double r = (d.throatDiameterMm +
                          (d.outletDiameterMm - d.throatDiameterMm) * smooth(t)) / 2.0;
        if (!result.addPoint({d.convergingLengthMm + d.divergingLengthMm * t, r}))
            return std::nullopt;
    }
    for (int i = segments - 1; i >= 0; --i) {
        const double t = static_cast<double>(i) / segments;
        const double r = (d.inletDiameterMm +
                          (d.throatDiameterMm - d.inletDiameterMm) * smooth(t)) / 2.0;
        if (!result.addPoint({d.convergingLengthMm * t, r}))
            return std::nullopt;
    }
    if (!result.close()) return std::nullopt;
    return result;
}

inline std::optional<Sketch2D> makeRectangleSketch(double widthMm, double heightMm) {
    if (!(widthMm > 0) || !(heightMm > 0) || !std::isfinite(widthMm) ||
        !std::isfinite(heightMm) || widthMm > 1e6 || heightMm > 1e6)
        return std::nullopt;
    Sketch2D s;
    if (!s.addPoint({0,0}) || !s.addPoint({widthMm,0}) ||
        !s.addPoint({widthMm,heightMm}) || !s.addPoint({0,heightMm}) ||
        !s.close()) return std::nullopt;
    return s;
}

inline std::optional<Sketch2D> makeCircleSketch(double diameterMm, int sides = 96) {
    if (!(diameterMm > 0) || !std::isfinite(diameterMm) ||
        diameterMm > 1e6 || sides < 12 || sides > 1024) return std::nullopt;
    Sketch2D s;
    constexpr double tau = 6.283185307179586476925286766559;
    for (int i = 0; i < sides; ++i) {
        const double angle = tau * i / sides;
        if (!s.addPoint({0.5 * diameterMm * std::cos(angle),
                         0.5 * diameterMm * std::sin(angle)}))
            return std::nullopt;
    }
    if (!s.close()) return std::nullopt;
    return s;
}

} // namespace vortia
