#pragma once

#include <cmath>
#include <cstddef>
#include <vector>
#include <optional>

namespace vortia {

struct SketchPoint2D {
    double xMm{0.0};
    double yMm{0.0};
};

struct SketchArc2D {
    SketchPoint2D center;
    double radiusMm;
    double startRadians;
    double sweepRadians;
};

/// Dimensioned planar path with straight segments and exact circular arcs.
/// Sketch edits are independent of CFD meshing until a supported 2D converter
/// is connected. This is NOT a general parametric constraint solver.
class Sketch2D {
public:
    [[nodiscard]] const std::vector<SketchPoint2D>& points() const { return points_; }
    [[nodiscard]] bool closed() const { return closed_; }
    [[nodiscard]] std::size_t segmentCount() const {
        return points_.size() < 2 ? 0 : points_.size() - 1 + (closed_ ? 1 : 0);
    }
    [[nodiscard]] bool addPoint(SketchPoint2D p) {
        if (closed_ || !finite(p) || points_.size() >= 10000)
            return false;
        if (!points_.empty() && distance(points_.back(), p) < 1e-6)
            return false;
        points_.push_back(p);
        bulges_.push_back(0.0);
        return true;
    }
    [[nodiscard]] bool movePoint(std::size_t index, SketchPoint2D p) {
        if (index >= points_.size() || !finite(p) || pointLockedByArcs(index))
            return false;
        if (index > 0 && distance(points_[index - 1], p) < 1e-6)
            return false;
        if (index + 1 < points_.size() && distance(points_[index + 1], p) < 1e-6)
            return false;
        if (closed_ && ((index == 0 && distance(points_.back(), p) < 1e-6)
                     || (index + 1 == points_.size() && distance(points_[0], p) < 1e-6)))
            return false;
        points_[index] = p;
        return true;
    }
    [[nodiscard]] bool close() {
        if (points_.size() < 3 || distance(points_.front(), points_.back()) < 1e-6)
            return false;
        closed_ = true;
        return true;
    }
    void open() { closed_ = false; if (!bulges_.empty()) bulges_.back() = 0.0; }
    void clear() { points_.clear(); bulges_.clear(); closed_ = false; }
    [[nodiscard]] double segmentLength(std::size_t index) const;
    [[nodiscard]] double segmentBulge(std::size_t index) const {
        return index < segmentCount() ? bulges_[index] : 0.0;
    }
    [[nodiscard]] bool hasArcs() const;
    [[nodiscard]] std::optional<SketchArc2D> arc(std::size_t index) const;
    [[nodiscard]] SketchPoint2D segmentPoint(std::size_t index, double t) const;
    /// File-format primitive: bulge = tan(signed arc sweep / 4). Zero is a line.
    /// Only minor arcs / semicircles are supported; invalid input changes nothing.
    [[nodiscard]] bool setSegmentBulge(std::size_t index, double bulge);
    /// Replace a corner between two lines with a tangent circular fillet.
    [[nodiscard]] bool filletVertex(std::size_t index, double radiusMm);
    /// Resize an unsplit fillet, retaining the two supporting tangent lines.
    [[nodiscard]] bool setFilletRadius(std::size_t segment, double radiusMm);
    [[nodiscard]] bool pointLockedByArcs(std::size_t index) const;
    /// Insert a vertex at a fractional position along a selected segment.
    [[nodiscard]] bool splitSegment(std::size_t index, double t = 0.5);
    /// Fix the length of a selected segment by moving its endpoint.
    /// Adjacent segments may change length; not a global constraint solver.
    [[nodiscard]] bool setSegmentLength(std::size_t index, double lengthMm) {
        if (index >= segmentCount() || segmentBulge(index) != 0.0
            || !std::isfinite(lengthMm) || lengthMm <= 0.0)
            return false;
        const auto a = points_[index];
        const std::size_t next = (index + 1) % points_.size();
        const auto b = points_[next];
        const double original = distance(a, b);
        if (original <= 1e-9) return false;
        const double k = lengthMm / original;
        return movePoint(next, {a.xMm + (b.xMm - a.xMm) * k,
                                a.yMm + (b.yMm - a.yMm) * k});
    }
    [[nodiscard]] double perimeter() const {
        double result = 0.0;
        for (std::size_t i = 0; i < segmentCount(); ++i)
            result += segmentLength(i);
        return result;
    }
    /// Signed exact area, including circular segments, in mm^2; closed paths only.
    [[nodiscard]] double signedArea() const;
private:
    static bool finite(SketchPoint2D p) {
        return std::isfinite(p.xMm) && std::isfinite(p.yMm);
    }
    static double distance(SketchPoint2D a, SketchPoint2D b) {
        return std::hypot(b.xMm - a.xMm, b.yMm - a.yMm);
    }
    std::vector<SketchPoint2D> points_;
    std::vector<double> bulges_;
    bool closed_{false};
};

} // namespace vortia
