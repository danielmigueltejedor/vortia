//! The distance from the cells beside a wall to the wall, as wall functions
//! need it.
//!
//! Derived from OpenFOAM's `nearWallDist` and `cellDistFuncs` (OpenFOAM,
//! Copyright (C) 2011-2018 OpenFOAM Foundation, GPL-3.0-or-later): for each
//! face of a patch, the distance from its cell's centre to the nearest point
//! of that face or of any face of the patch sharing a point with it. The
//! nearest point of a face is found on the triangles it splits into around its
//! centre, with the closest-point-on-triangle test OpenFOAM uses, so the
//! distances agree with OpenFOAM's to rounding.

use std::collections::HashMap;

use vortia_core::Vec3;

use crate::polymesh::{Patch, PolyMesh};

const VSMALL: f64 = 1.0e-300;

/// OpenFOAM's `face::centre`: the area-weighted centre of the triangles
/// around the points' average. Not the same arithmetic as the mesh's face
/// centres, so kept apart.
fn face_centre(pts: &[Vec3]) -> Vec3 {
    match pts {
        [] => Vec3::default(),
        [a, b, c] => (*a + *b + *c) * (1.0 / 3.0),
        _ => {
            let mut centre = Vec3::default();
            for &p in pts {
                centre = centre + p;
            }
            centre = centre / pts.len() as f64;
            let (mut sum_a, mut sum_ac) = (0.0, Vec3::default());
            for (i, &p) in pts.iter().enumerate() {
                let next = pts.get(i + 1).or(pts.first()).copied().unwrap_or(p);
                let ttc = p + next + centre;
                let ta = (p - centre).cross(next - centre).norm();
                sum_a += ta;
                sum_ac = sum_ac + ttc * ta;
            }
            if sum_a > VSMALL {
                sum_ac / (3.0 * sum_a)
            } else {
                centre
            }
        }
    }
}

/// The distance from `p` to the triangle `abc` (Ericson's closest point on a
/// triangle, in OpenFOAM's arithmetic).
fn triangle_distance(a: Vec3, b: Vec3, c: Vec3, p: Vec3) -> f64 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a - p).norm();
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (b - p).norm();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v - p).norm();
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (c - p).norm();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w - p).norm();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w - p).norm();
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (a + ab * v + ac * w - p).norm()
}

/// The distance from `p` to the nearest point of a face.
fn face_distance(pts: &[Vec3], p: Vec3) -> f64 {
    let centre = face_centre(pts);
    let mut nearest = f64::MAX;
    for (i, &a) in pts.iter().enumerate() {
        let next = pts.get(i + 1).or(pts.first()).copied().unwrap_or(a);
        let d = triangle_distance(a, next, centre, p).abs();
        if d < nearest {
            nearest = d;
        }
    }
    nearest
}

/// For every face of `patch`, the distance from the centre of the cell it
/// belongs to (`cell_centres`) to the nearest of that face and the patch's
/// faces that share a point with it.
pub fn near_wall_distance(mesh: &PolyMesh, patch: &Patch, cell_centres: &[Vec3]) -> Vec<f64> {
    let mut by_point: HashMap<usize, Vec<usize>> = HashMap::new();
    for f in patch.faces() {
        for &p in mesh.face(f) {
            by_point.entry(p).or_default().push(f);
        }
    }
    let mut pts: Vec<Vec3> = Vec::new();
    let mut out = Vec::with_capacity(patch.size);
    for f in patch.faces() {
        let centre = mesh
            .owner()
            .get(f)
            .and_then(|&c| cell_centres.get(c))
            .copied()
            .unwrap_or_default();
        let mut near: Vec<usize> = mesh
            .face(f)
            .iter()
            .filter_map(|p| by_point.get(p))
            .flatten()
            .copied()
            .collect();
        near.sort_unstable();
        near.dedup();
        let mut smallest = f64::MAX;
        for g in near {
            pts.clear();
            pts.extend(mesh.face_points(g));
            let d = face_distance(&pts, centre);
            if d < smallest {
                smallest = d;
            }
        }
        out.push(smallest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::structured_box;
    use crate::geometry::MeshGeometry;
    use vortia_core::Aabb;

    #[test]
    fn a_box_measures_half_a_cell_to_its_sides() {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 0.1, 0.01),
        };
        let mesh = structured_box(bounds, [40, 10, 1]).unwrap();
        let geo = MeshGeometry::compute(&mesh);
        let y_min = mesh.patch("yMin").unwrap();
        let d = near_wall_distance(&mesh, y_min, &geo.cell_centres);
        assert_eq!(d.len(), 40);
        for v in d {
            assert!((v - 0.005).abs() < 1e-15, "{v}");
        }
        // A point off the corner of a face is nearest to the corner.
        let square = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let d = face_distance(&square, Vec3::new(2.0, 2.0, 0.0));
        assert!((d - 2f64.sqrt()).abs() < 1e-15);
    }
}
