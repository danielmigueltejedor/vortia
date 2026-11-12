//! Curved block edges: `line`, `arc`, `polyLine`, `spline` (Catmull-Rom),
//! `BSpline` and `bezier`, as `blockMeshDict`'s `edges` lists them.
//!
//! Derived from OpenFOAM's `blockEdges` (GPL-3.0-or-later, like Vortia). An
//! edge is a curve from one block vertex to another; the block divides it
//! with its grading and the transfinite interpolation then bends the block's
//! inside towards it.

use vortia_core::Vec3;
use vortia_foamfile::{Dictionary, Entry, Value};

use super::names::Names;
use crate::error::{MeshError, Result};

/// OpenFOAM's `SMALL`, the tolerance of the end-point tests.
const SMALL: f64 = 1e-15;

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

/// The most knots one edge may have.
const MAX_KNOTS: usize = 1 << 20;

/// A curve through some points, parametrised by the fraction of its length
/// at each knot (`polyLine`, which the splines build on).
#[derive(Debug, Clone)]
pub(super) struct PolyLine {
    points: Vec<Vec3>,
    param: Vec<f64>,
}

impl PolyLine {
    fn new(points: Vec<Vec3>) -> Result<Self> {
        let mut param = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, p) in points.iter().enumerate() {
            if let Some(prev) = i.checked_sub(1).and_then(|j| points.get(j)) {
                total += (*p - *prev).norm();
            }
            param.push(total);
        }
        if points.len() < 2 || !(total > 0.0 && total.is_finite()) {
            return invalid("an edge through points that all coincide has no length");
        }
        let n = param.len();
        for (i, p) in param.iter_mut().enumerate() {
            if i > 0 && i + 1 < n {
                *p /= total;
            }
        }
        if let Some(last) = param.last_mut() {
            *last = 1.0;
        }
        Ok(Self { points, param })
    }

    fn n_segments(&self) -> usize {
        self.points.len().saturating_sub(1)
    }

    fn first(&self) -> Vec3 {
        self.points.first().copied().unwrap_or_default()
    }

    fn last(&self) -> Vec3 {
        self.points.last().copied().unwrap_or_default()
    }

    fn knot(&self, i: usize) -> Vec3 {
        self.points.get(i).copied().unwrap_or_default()
    }

    /// `polyLine::localParameter`: the segment `lambda` falls on, and where
    /// on it.
    fn local_parameter(&self, lambda: f64) -> (usize, f64) {
        if lambda < SMALL {
            return (0, 0.0);
        }
        if lambda > 1.0 - SMALL {
            return (self.n_segments(), 1.0);
        }
        let mut segment = 1;
        while self.param.get(segment).is_some_and(|&p| p < lambda) {
            segment += 1;
        }
        segment -= 1;
        let lo = self.param.get(segment).copied().unwrap_or(0.0);
        let hi = self.param.get(segment + 1).copied().unwrap_or(1.0);
        (segment, (lambda - lo) / (hi - lo))
    }

    fn line_at(&self, segment: usize, mu: f64) -> Vec3 {
        if segment > self.n_segments() {
            return self.last();
        }
        let p0 = self.knot(segment);
        let p1 = self.knot(segment + 1);
        if mu <= 0.0 {
            p0
        } else if mu >= 1.0 {
            p1
        } else {
            p0 + (p1 - p0) * mu
        }
    }

    /// The knots on each side of a segment, the ends reflected
    /// (`CatmullRomSpline`, `BSpline`).
    fn neighbours(&self, segment: usize) -> (Vec3, Vec3) {
        let p0 = self.knot(segment);
        let p1 = self.knot(segment + 1);
        let e0 = match segment.checked_sub(1) {
            None => p0 * 2.0 - p1,
            Some(i) => self.knot(i),
        };
        let e1 = if segment + 1 == self.n_segments() {
            p1 * 2.0 - p0
        } else {
            self.knot(segment + 2)
        };
        (e0, e1)
    }

    fn catmull_rom(&self, mu: f64) -> Vec3 {
        if mu < SMALL {
            return self.first();
        }
        if mu > 1.0 - SMALL {
            return self.last();
        }
        let (segment, mu) = self.local_parameter(mu);
        if segment > self.n_segments() {
            return self.last();
        }
        let p0 = self.knot(segment);
        let p1 = self.knot(segment + 1);
        if mu <= 0.0 {
            return p0;
        }
        if mu >= 1.0 {
            return p1;
        }
        let (e0, e1) = self.neighbours(segment);
        let c3 = (e0 * -1.0 + p0 * 3.0) - p1 * 3.0 + e1;
        let c2 = (e0 * 2.0 - p0 * 5.0) + p1 * 4.0 - e1;
        let c1 = e0 * -1.0 + p1;
        ((p0 * 2.0) + (c1 + (c2 + c3 * mu) * mu) * mu) * 0.5
    }

    fn b_spline(&self, mu: f64) -> Vec3 {
        if mu < SMALL {
            return self.first();
        }
        if mu > 1.0 - SMALL {
            return self.last();
        }
        let (segment, mu) = self.local_parameter(mu);
        if segment > self.n_segments() {
            return self.last();
        }
        let p0 = self.knot(segment);
        let p1 = self.knot(segment + 1);
        if segment == 0 && mu <= 0.0 {
            return p0;
        }
        if segment + 1 == self.n_segments() && mu >= 1.0 {
            return p1;
        }
        let (e0, e1) = self.neighbours(segment);
        let c3 = (e0 * -1.0 + p0 * 3.0) - p1 * 3.0 + e1;
        let c2 = (e0 * 3.0 - p0 * 6.0) + p1 * 3.0;
        let c1 = e0 * -3.0 + p1 * 3.0;
        let c0 = (e0 + p0 * 4.0) + p1;
        (c0 + (c1 + (c2 + c3 * mu) * mu) * mu) * (1.0 / 6.0)
    }
}

/// The three points of an `arc`, with the circle through them.
#[derive(Debug, Clone)]
pub(super) struct Arc {
    p1: Vec3,
    p3: Vec3,
    centre: Vec3,
    radius: f64,
    /// Degrees, as `arcEdge` keeps it.
    angle: f64,
    /// The columns of the rotation of the arc's own frame: the direction of
    /// the first radius, the tangent at it, and the axis.
    e1: Vec3,
    e2: Vec3,
    e3: Vec3,
}

impl Arc {
    /// `arcEdge::calcAngle` and the cylindrical frame it makes.
    fn new(p1: Vec3, p2: Vec3, p3: Vec3) -> Result<Self> {
        let a = p2 - p1;
        let b = p3 - p1;
        let asqr = a.dot(a);
        let bsqr = b.dot(b);
        let adotb = a.dot(b);
        let denom = asqr * bsqr - adotb * adotb;
        if denom.abs() < 1e-300 || !denom.is_finite() {
            return invalid("the three points of an arc are on a line");
        }
        let fact = 0.5 * (bsqr - adotb) / denom;
        let centre = (a * 0.5 + a.cross(b).cross(a) * fact) + p1;
        let r1 = p1 - centre;
        let r2 = p2 - centre;
        let r3 = p3 - centre;
        let cosine = r3.dot(r1) / (r3.norm() * r1.norm());
        let mut angle = cosine.acos() * 180.0 / std::f64::consts::PI;
        if !angle.is_finite() {
            return invalid("the three points of an arc do not define a circle");
        }
        if r1.cross(r2).dot(r1.cross(r3)) < 0.0 {
            angle = 360.0 - angle;
        }
        let axis = if angle <= 180.0 {
            let axis = r1.cross(r3);
            if axis.norm() / (r1.norm() * r3.norm()) < 0.001 {
                r1.cross(r2)
            } else {
                axis
            }
        } else {
            r3.cross(r1)
        };
        let radius = r3.norm();

        // `coordinateRotations::axes(axis, r1)`, the axis first.
        let mag_axis = axis.norm();
        if mag_axis.is_nan() || mag_axis < 1e-150 {
            return invalid("an arc whose points are on a line has no plane");
        }
        let e3 = axis / mag_axis;
        let mut e1 = r1;
        e1 = e1 - e3 * e3.dot(e1);
        let mag1 = e1.norm();
        if mag1.is_nan() || mag1 < SMALL {
            return invalid("an arc's first radius is along its axis");
        }
        e1 = e1 / mag1;
        let e2 = e3.cross(e1);
        Ok(Self {
            p1,
            p3,
            centre,
            radius,
            angle,
            e1,
            e2,
            e3,
        })
    }

    fn position(&self, lambda: f64) -> Vec3 {
        if lambda < SMALL {
            return self.p1;
        }
        if lambda > 1.0 - SMALL {
            return self.p3;
        }
        let theta = lambda * self.angle * std::f64::consts::PI / 180.0;
        let (vx, vy, vz) = (self.radius * theta.cos(), self.radius * theta.sin(), 0.0);
        Vec3::new(
            self.e1.x * vx + self.e2.x * vy + self.e3.x * vz,
            self.e1.y * vx + self.e2.y * vy + self.e3.y * vz,
            self.e1.z * vx + self.e2.z * vy + self.e3.z * vz,
        ) + self.centre
    }
}

#[derive(Debug, Clone)]
pub(super) enum Curve {
    Line { from: Vec3, to: Vec3 },
    Arc(Arc),
    PolyLine(PolyLine),
    Spline(PolyLine),
    BSpline(PolyLine),
    Bezier(Vec<Vec3>),
}

/// One entry of `edges`: a curve between two block vertices.
#[derive(Debug, Clone)]
pub(super) struct CurvedEdge {
    pub start: usize,
    pub end: usize,
    curve: Curve,
}

impl CurvedEdge {
    /// 1 when the edge runs `start` to `end`, -1 when it runs the other way,
    /// 0 when it is some other edge (`blockEdge::compare`).
    pub fn compare(&self, start: usize, end: usize) -> i32 {
        if self.start == start && self.end == end {
            1
        } else if self.start == end && self.end == start {
            -1
        } else {
            0
        }
    }

    /// The point at fraction `lambda` of the curve (`position`).
    pub fn position(&self, lambda: f64) -> Vec3 {
        match &self.curve {
            Curve::Line { from, to } => *from + (*to - *from) * lambda,
            Curve::Arc(a) => a.position(lambda),
            Curve::PolyLine(p) => {
                if lambda < SMALL {
                    p.first()
                } else if lambda > 1.0 - SMALL {
                    p.last()
                } else {
                    let (segment, mu) = p.local_parameter(lambda);
                    p.line_at(segment, mu)
                }
            }
            Curve::Spline(p) => p.catmull_rom(lambda),
            Curve::BSpline(p) => p.b_spline(lambda),
            Curve::Bezier(control) => {
                let mut working = control.clone();
                while working.len() > 1 {
                    working = working
                        .windows(2)
                        .map(|w| match w {
                            [a, b] => *a * (1.0 - lambda) + *b * lambda,
                            _ => Vec3::default(),
                        })
                        .collect();
                }
                working.first().copied().unwrap_or_default()
            }
        }
    }
}

fn vertex_label(v: &Value, n_vertices: usize, names: &Names) -> Result<usize> {
    if matches!(v, Value::Str(_)) {
        return invalid("an edge's end is not a vertex of the dictionary");
    }
    let label = match v {
        Value::Word(_) => names.vertex(v, "an edge's end")?,
        _ => v
            .as_label()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(usize::MAX),
    };
    if label >= n_vertices {
        return invalid("an edge's end is not a vertex of the dictionary");
    }
    Ok(label)
}

/// The entries of `edges`, with the one that a counted-list reading put back.
struct Cursor<'a> {
    items: std::slice::Iter<'a, Value>,
    pending: Option<&'a Value>,
}

impl Cursor<'_> {
    fn peek_is_list(&self) -> bool {
        match self.pending {
            Some(v) => v.as_list().is_some(),
            None => self
                .items
                .clone()
                .next()
                .is_some_and(|v| v.as_list().is_some()),
        }
    }
}

fn take<'a>(it: &mut Cursor<'a>, kind: &str, what: &str) -> Result<&'a Value> {
    it.pending.take().or_else(|| it.items.next()).map_or_else(
        || invalid(format!("the {kind} edge is missing its {what}")),
        Ok,
    )
}

fn point(v: &Value) -> Result<Vec3> {
    match v.as_list().map(<[Value]>::len) {
        Some(3) => {}
        _ => return invalid("an edge point is (x y z)"),
    }
    let n = |i: usize| {
        v.as_list()
            .and_then(|l| l.get(i))
            .and_then(Value::as_scalar)
            .filter(|x| x.is_finite())
    };
    match (n(0), n(1), n(2)) {
        (Some(x), Some(y), Some(z)) => Ok(Vec3::new(x, y, z)),
        _ => invalid("an edge point is three numbers"),
    }
}

fn points(v: &Value) -> Result<Vec<Vec3>> {
    let items = v
        .as_list()
        .map_or_else(|| invalid("an edge's points are a list of (x y z)"), Ok)?;
    if items.len() > MAX_KNOTS {
        return invalid("too many points on an edge");
    }
    items.iter().map(point).collect()
}

/// The curved edges of a `blockMeshDict`, in the order they are listed.
pub(super) fn read(dict: &Dictionary, verts: &[Vec3], names: &Names) -> Result<Vec<CurvedEdge>> {
    let items: &[Value] = match dict.get("edges") {
        None => return Ok(Vec::new()),
        Some(Entry::Values(v)) => match v.as_slice() {
            [first] => first
                .as_list()
                .map_or_else(|| invalid("`edges` is a list"), Ok)?,
            [] => return Ok(Vec::new()),
            _ => return invalid("`edges` is one list"),
        },
        Some(_) => return invalid("`edges` is a list"),
    };
    let at = |label: usize| verts.get(label).copied().unwrap_or_default();
    let mut out = Vec::new();
    let mut it = Cursor {
        items: items.iter(),
        pending: None,
    };
    while let Some(kind) = it.items.next() {
        let Some(kind) = kind.as_word() else {
            return invalid("an edge starts with its type (arc, polyLine, spline, …)");
        };
        let start = vertex_label(take(&mut it, kind, "first vertex")?, verts.len(), names)?;
        let end_value = take(&mut it, kind, "second vertex")?;
        // `3 (…)` reads as a counted list of three: here the 3 is the vertex
        // and the list is the edge's next argument.
        let end = match end_value.as_list() {
            Some(l) => {
                it.pending = Some(end_value);
                vertex_label(&Value::Label(l.len() as i64), verts.len(), names)?
            }
            None => vertex_label(end_value, verts.len(), names)?,
        };
        if start == end {
            return invalid(format!("a {kind} edge from vertex {start} to itself"));
        }
        let (a, b) = (at(start), at(end));
        let knots = |inner: Vec<Vec3>| {
            let mut all = Vec::with_capacity(inner.len() + 2);
            all.push(a);
            all.extend(inner);
            all.push(b);
            all
        };
        let curve = match kind {
            "line" => Curve::Line { from: a, to: b },
            "arc" => Curve::Arc(Arc::new(
                a,
                point(take(&mut it, kind, "middle point")?)?,
                b,
            )?),
            "polyLine" => Curve::PolyLine(PolyLine::new(knots(points(take(
                &mut it, kind, "points",
            )?)?))?),
            "bezier" => Curve::Bezier(knots(points(take(&mut it, kind, "control points")?)?)),
            "spline" | "BSpline" => {
                let spline = PolyLine::new(knots(points(take(&mut it, kind, "points")?)?))?;
                // Two unused end tangents may follow.
                if it.peek_is_list() {
                    for _ in 0..2 {
                        point(take(&mut it, kind, "tangent")?)?;
                    }
                }
                if kind == "spline" {
                    Curve::Spline(spline)
                } else {
                    Curve::BSpline(spline)
                }
            }
            other => {
                return invalid(format!(
                    "the {other} edge is not supported yet (line, arc, polyLine, spline, \
                     BSpline and bezier are)"
                ));
            }
        };
        out.push(CurvedEdge { start, end, curve });
        if out.len() > MAX_KNOTS {
            return invalid("too many curved edges");
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).norm() < 1e-12
    }

    #[test]
    fn an_arc_runs_through_its_middle_point_on_a_circle() {
        let (a, m, b) = (
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
        );
        let arc = Arc::new(a, m, b).unwrap();
        assert!(close(arc.position(0.5), m));
        assert!(close(arc.position(0.0), a));
        assert!(close(arc.position(1.0), b));
        for k in 1..10 {
            let p = arc.position(f64::from(k) / 10.0);
            assert!((p.norm() - 1.0).abs() < 1e-12 && p.z.abs() < 1e-12);
        }
    }

    #[test]
    fn a_polyline_and_the_splines_pass_through_their_knots() {
        let knots = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(3.0, 1.0, 1.0),
            Vec3::new(4.0, 0.0, 1.0),
        ];
        let line = PolyLine::new(knots.clone()).unwrap();
        for (i, k) in knots.iter().enumerate() {
            let at = line.param[i];
            let poly = CurvedEdge {
                start: 0,
                end: 1,
                curve: Curve::PolyLine(line.clone()),
            };
            assert!(close(poly.position(at), *k), "polyLine at knot {i}");
            // Catmull-Rom interpolates its knots; the B-spline does not
            // have to, but both start and end on the first and last.
            assert!(close(line.catmull_rom(at), *k), "spline at knot {i}");
        }
        assert!(close(line.b_spline(0.0), knots[0]));
        assert!(close(line.b_spline(1.0), knots[3]));
    }

    #[test]
    fn a_bezier_curve_of_three_points_is_the_quadratic() {
        let c = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ];
        let edge = CurvedEdge {
            start: 0,
            end: 1,
            curve: Curve::Bezier(c),
        };
        assert!(close(edge.position(0.5), Vec3::new(1.0, 1.0, 0.0)));
    }

    #[test]
    fn edges_that_cannot_be_curves_are_errors() {
        let p = Vec3::new(0.0, 0.0, 0.0);
        assert!(Arc::new(p, Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)).is_err());
        assert!(PolyLine::new(vec![p, p]).is_err());
        assert!(PolyLine::new(vec![p]).is_err());
    }

    #[test]
    fn compare_tells_the_direction() {
        let e = CurvedEdge {
            start: 2,
            end: 5,
            curve: Curve::Line {
                from: Vec3::default(),
                to: Vec3::default(),
            },
        };
        assert_eq!(e.compare(2, 5), 1);
        assert_eq!(e.compare(5, 2), -1);
        assert_eq!(e.compare(2, 3), 0);
    }
}
