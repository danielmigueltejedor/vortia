#pragma once

#include <cmath>
#include <cstddef>
#include <vector>

namespace vortia {

struct SketchPoint2D {
    double xMm{0.0};
    double yMm{0.0};
};

/// Dimensioned planar polyline; coordinates are explicitly in millimetres.
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
        return true;
    }
    [[nodiscard]] bool movePoint(std::size_t index, SketchPoint2D p) {
        if (index >= points_.size() || !finite(p))
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
    void open() { closed_ = false; }
    void clear() { points_.clear(); closed_ = false; }
    [[nodiscard]] double segmentLength(std::size_t index) const {
        if (index >= segmentCount()) return 0.0;
        return distance(points_[index], points_[(index + 1) % points_.size()]);
    }
    /// Insert a vertex at a fractional position along a selected segment.
    [[nodiscard]] bool splitSegment(std::size_t index, double t = 0.5) {
        if (index >= segmentCount() || !std::isfinite(t) || t <= 0.0 || t >= 1.0)
            return false;
        const auto a = points_[index];
        const auto b = points_[(index + 1) % points_.size()];
        const SketchPoint2D p{a.xMm + t * (b.xMm - a.xMm),
                              a.yMm + t * (b.yMm - a.yMm)};
        points_.insert(points_.begin() + static_cast<std::ptrdiff_t>(index + 1), p);
        return true;
    }
    /// Fix the length of a selected segment by moving its endpoint.
    /// Adjacent segments may change length; not a global constraint solver.
    [[nodiscard]] bool setSegmentLength(std::size_t index, double lengthMm) {
        if (index >= segmentCount() || !std::isfinite(lengthMm) || lengthMm <= 0.0)
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
    /// Signed planar area in mm^2, available only for a closed profile.
    [[nodiscard]] double signedArea() const {
        if (!closed_) return 0.0;
        double twiceArea = 0.0;
        for (std::size_t i = 0; i < points_.size(); ++i) {
            const auto& a = points_[i];
            const auto& b = points_[(i + 1) % points_.size()];
            twiceArea += a.xMm * b.yMm - b.xMm * a.yMm;
        }
        return twiceArea * 0.5;
    }
private:
    static bool finite(SketchPoint2D p) {
        return std::isfinite(p.xMm) && std::isfinite(p.yMm);
    }
    static double distance(SketchPoint2D a, SketchPoint2D b) {
        return std::hypot(b.xMm - a.xMm, b.yMm - a.yMm);
    }
    std::vector<SketchPoint2D> points_;
    bool closed_{false};
};

} // namespace vortia
