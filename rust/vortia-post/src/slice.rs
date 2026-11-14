//! A slice through the volume: where a plane cuts the cells of the mesh, one polygon per cell
//! cut, so a field can be looked at inside the domain and not only on its boundary.
//!
//! A polygon is the section of one cell, taken as convex (the cells of a CFD mesh are): the
//! points where the plane crosses the edges of the cell's faces, in order around their middle.

use vortia_core::Vec3;

use crate::case::Case;
use crate::error::{PostError, Result};

/// The section of one cell by the plane.
#[derive(Debug, Clone, PartialEq)]
pub struct SlicePolygon {
    pub cell: usize,
    /// At least three points, in order around the polygon.
    pub points: Vec<Vec3>,
}

impl Case {
    /// The cells the plane through `origin` with direction `normal` cuts, each with its section.
    /// A cell the plane only touches is left out; when the plane lies along the faces between
    /// cells, the cells on both sides are cut, and both show the same face.
    pub fn slice(&self, origin: Vec3, normal: Vec3) -> Result<Vec<SlicePolygon>> {
        let length = normal.norm();
        let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
        if !finite(origin) || !finite(normal) || !length.is_normal() {
            return Err(PostError::Invalid(
                "a slice needs a point and a direction that are numbers, and a direction that is not zero"
                    .into(),
            ));
        }
        let n = normal * (1.0 / length);
        // Two vectors across the plane, to put the points in order.
        let helper = if n.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let u = cross(n, helper);
        let u = u * (1.0 / u.norm());
        let v = cross(n, u);

        let points = self.mesh.points();
        let mut out = Vec::new();
        for (cell, faces) in self.cell_faces.iter().enumerate() {
            let size = self
                .geometry
                .cell_volumes
                .get(cell)
                .map_or(0.0, |v| v.abs().cbrt());
            let eps = 1.0e-9 * size;
            let distance = |p: Vec3| (p - origin).dot(n);

            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for &f in faces {
                for &i in self.mesh.face(f) {
                    if let Some(&p) = points.get(i) {
                        let d = distance(p);
                        low = low.min(d);
                        high = high.max(d);
                    }
                }
            }
            if !(low.is_finite() && high.is_finite()) || low > eps || high < -eps {
                continue;
            }

            let mut cut: Vec<Vec3> = Vec::new();
            let mut add = |p: Vec3| {
                if cut
                    .iter()
                    .all(|q| (*q - p).norm() > eps.max(1.0e-12 * size))
                {
                    cut.push(p);
                }
            };
            for &f in faces {
                let ring = self.mesh.face(f);
                for (k, &i) in ring.iter().enumerate() {
                    let j = ring.get((k + 1) % ring.len()).copied().unwrap_or(i);
                    let (Some(&a), Some(&b)) = (points.get(i), points.get(j)) else {
                        continue;
                    };
                    let (da, db) = (distance(a), distance(b));
                    if da.abs() <= eps {
                        add(a);
                    }
                    if (da < -eps && db > eps) || (da > eps && db < -eps) {
                        add(a + (b - a) * (da / (da - db)));
                    }
                }
            }
            if cut.len() < 3 {
                continue;
            }
            let middle =
                cut.iter().fold(Vec3::new(0.0, 0.0, 0.0), |s, &p| s + p) * (1.0 / cut.len() as f64);
            cut.sort_by(|&a, &b| {
                let angle = |p: Vec3| {
                    let d = p - middle;
                    d.dot(v).atan2(d.dot(u))
                };
                angle(a).total_cmp(&angle(b))
            });
            out.push(SlicePolygon { cell, points: cut });
        }
        Ok(out)
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
