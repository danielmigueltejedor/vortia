#include "vortia/geometry/Sketch2D.hpp"

#include <algorithm>

namespace vortia {
namespace {
using P = SketchPoint2D;
P add(P a, P b) { return {a.xMm + b.xMm, a.yMm + b.yMm}; }
P sub(P a, P b) { return {a.xMm - b.xMm, a.yMm - b.yMm}; }
P mul(P a, double k) { return {a.xMm * k, a.yMm * k}; }
double cross(P a, P b) { return a.xMm * b.yMm - a.yMm * b.xMm; }
double dot(P a, P b) { return a.xMm * b.xMm + a.yMm * b.yMm; }
double norm(P a) { return std::hypot(a.xMm, a.yMm); }
bool finite(P p) { return std::isfinite(p.xMm) && std::isfinite(p.yMm); }

struct Fillet { P a; P b; double bulge; };

std::optional<Fillet> makeFillet(P previous, P corner, P next, double radius) {
    if (!std::isfinite(radius) || radius < 0.001 || radius > 1e6) return {};
    const double left = norm(sub(previous, corner));
    const double right = norm(sub(next, corner));
    if (left < 1e-6 || right < 1e-6) return {};
    const P u = mul(sub(previous, corner), 1.0 / left);
    const P v = mul(sub(next, corner), 1.0 / right);
    const double turn = std::atan2(cross(mul(u, -1), v), dot(mul(u, -1), v));
    const double theta = std::acos(std::clamp(dot(u, v), -1.0, 1.0));
    if (theta < 1e-6 || std::abs(turn) < 1e-6) return {};
    const double offset = radius / std::tan(theta * 0.5);
    if (!std::isfinite(offset) || offset < 1e-6 || offset >= left - 1e-6 || offset >= right - 1e-6)
        return {};
    const Fillet result{add(corner, mul(u, offset)), add(corner, mul(v, offset)), std::tan(turn * 0.25)};
    if (!finite(result.a) || !finite(result.b)) return {};
    return result;
}
} // namespace

bool Sketch2D::hasArcs() const {
    for (std::size_t i = 0; i < segmentCount(); ++i)
        if (bulges_[i] != 0.0) return true;
    return false;
}

std::optional<SketchArc2D> Sketch2D::arc(std::size_t index) const {
    if (index >= segmentCount() || bulges_[index] == 0.0) return {};
    const double b = bulges_[index];
    const P start = points_[index];
    const P end = points_[(index + 1) % points_.size()];
    const P chord = sub(end, start);
    const P center = add(mul(add(start, end), 0.5),
                         mul({-chord.yMm, chord.xMm}, (1.0 - b*b) / (4.0*b)));
    const double radius = norm(chord) * (1.0 + b*b) / (4.0 * std::abs(b));
    if (!finite(center) || !std::isfinite(radius) || radius > 1e9 || radius < 1e-6) return {};
    return SketchArc2D{center, radius,
        std::atan2(start.yMm - center.yMm, start.xMm - center.xMm), 4.0 * std::atan(b)};
}

bool Sketch2D::setSegmentBulge(std::size_t index, double bulge) {
    if (index >= segmentCount() || !std::isfinite(bulge) || std::abs(bulge) > 1.0
        || (bulge != 0.0 && std::abs(bulge) < 1e-8)) return false;
    const double previous = bulges_[index];
    bulges_[index] = bulge;
    if (bulge != 0.0 && !arc(index)) { bulges_[index] = previous; return false; }
    return true;
}

SketchPoint2D Sketch2D::segmentPoint(std::size_t index, double t) const {
    if (index >= segmentCount() || !std::isfinite(t)) return {};
    const P a = points_[index];
    const P b = points_[(index + 1) % points_.size()];
    if (t <= 0) return a;
    if (t >= 1) return b;
    if (const auto c = arc(index)) {
        const double angle = c->startRadians + t * c->sweepRadians;
        return {c->center.xMm + c->radiusMm * std::cos(angle),
                c->center.yMm + c->radiusMm * std::sin(angle)};
    }
    return add(a, mul(sub(b, a), t));
}

double Sketch2D::segmentLength(std::size_t index) const {
    if (index >= segmentCount()) return 0;
    if (const auto c = arc(index)) return c->radiusMm * std::abs(c->sweepRadians);
    return distance(points_[index], points_[(index + 1) % points_.size()]);
}

double Sketch2D::signedArea() const {
    if (!closed_) return 0;
    double area = 0;
    for (std::size_t i = 0; i < segmentCount(); ++i) {
        area += 0.5 * cross(points_[i], points_[(i + 1) % points_.size()]);
        if (const auto c = arc(i))
            area += 0.5 * c->radiusMm * c->radiusMm * (c->sweepRadians - std::sin(c->sweepRadians));
    }
    return area;
}

bool Sketch2D::pointLockedByArcs(std::size_t index) const {
    // A raw coordinate edit could break either the arc or its tangent supports.
    // Until a global constraint solver exists, use the radius editor instead.
    const auto n = points_.size();
    for (std::size_t i = 0; i < segmentCount(); ++i) {
        if (bulges_[i] == 0) continue;
        if (index == i || index == (i + 1) % n
            || ((closed_ || i > 0) && index == (i + n - 1) % n)
            || ((closed_ || i + 2 < n) && index == (i + 2) % n)) return true;
    }
    return false;
}

bool Sketch2D::splitSegment(std::size_t index, double t) {
    if (index >= segmentCount() || points_.size() >= 10000 || !std::isfinite(t) || t <= 0 || t >= 1)
        return false;
    const P p = segmentPoint(index, t);
    const P a = points_[index], b = points_[(index + 1) % points_.size()];
    if (!finite(p) || distance(p, a) < 1e-6 || distance(p, b) < 1e-6) return false;
    const double sweep = 4.0 * std::atan(bulges_[index]);
    const double left = std::tan(sweep * t * 0.25);
    const double right = std::tan(sweep * (1.0 - t) * 0.25);
    if ((sweep != 0) && (std::abs(left) < 1e-8 || std::abs(right) < 1e-8)) return false;
    points_.insert(points_.begin() + static_cast<std::ptrdiff_t>(index + 1), p);
    bulges_[index] = left;
    bulges_.insert(bulges_.begin() + static_cast<std::ptrdiff_t>(index + 1), right);
    return true;
}

bool Sketch2D::filletVertex(std::size_t index, double radiusMm) {
    const auto n = points_.size();
    if (n < 3 || n >= 10000 || index >= n || (!closed_ && (index == 0 || index + 1 == n)))
        return false;
    const auto previous = (index + n - 1) % n;
    const auto next = (index + 1) % n;
    if (bulges_[previous] != 0 || bulges_[index] != 0) return false;
    const auto f = makeFillet(points_[previous], points_[index], points_[next], radiusMm);
    if (!f) return false;
    points_[index] = f->a;
    points_.insert(points_.begin() + static_cast<std::ptrdiff_t>(index + 1), f->b);
    bulges_[index] = f->bulge;
    bulges_.insert(bulges_.begin() + static_cast<std::ptrdiff_t>(index + 1), 0.0);
    return true;
}

bool Sketch2D::setFilletRadius(std::size_t segment, double radiusMm) {
    const auto n = points_.size();
    if (!arc(segment) || (!closed_ && (segment == 0 || segment + 2 >= n))) return false;
    const auto previous = (segment + n - 1) % n;
    const auto end = (segment + 1) % n;
    const auto next = (segment + 2) % n;
    if (bulges_[previous] != 0 || bulges_[end] != 0) return false;
    const P u = sub(points_[segment], points_[previous]);
    const P v = sub(points_[next], points_[end]);
    const double denominator = cross(u, v);
    if (std::abs(denominator) < 1e-10 * norm(u) * norm(v)) return false;
    const P corner = add(points_[previous], mul(u,
        cross(sub(points_[end], points_[previous]), v) / denominator));
    const auto f = makeFillet(points_[previous], corner, points_[next], radiusMm);
    if (!f) return false;
    // Imported arbitrary arcs may not be tangent fillets. Never silently turn
    // one into a different shape when the user asks to change its radius.
    const auto current = arc(segment).value();
    const auto original = makeFillet(points_[previous], corner, points_[next], current.radiusMm);
    const double tolerance = 1e-7 * std::max(1.0, current.radiusMm);
    if (!original || distance(original->a, points_[segment]) > tolerance
        || distance(original->b, points_[end]) > tolerance
        || std::abs(original->bulge - bulges_[segment]) > 1e-7) return false;
    points_[segment] = f->a;
    points_[end] = f->b;
    bulges_[segment] = f->bulge;
    return true;
}
} // namespace vortia
