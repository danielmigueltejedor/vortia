//! Distances to walls: from the cells beside a wall, as wall functions need
//! it, and from every cell, as the k-ω SST model's blending needs it.
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

use crate::polymesh::{Patch, PatchKind, PolyMesh};

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

/// The distance of every cell and every boundary face to the nearest wall,
/// as OpenFOAM's `wallDist` with `method meshWave` gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct WallDistance {
    pub cells: Vec<f64>,
    /// For every boundary face, in face order; `None` on empty patches.
    pub faces: Vec<Option<f64>>,
}

/// OpenFOAM's `SMALL`.
const SMALL: f64 = 1.0e-15;

/// What a wave carries to a cell or face: the centre of a wall face and the
/// squared distance to it, negative while unset.
#[derive(Debug, Clone, Copy)]
struct Nearest {
    origin: Vec3,
    dist2: f64,
}

impl Nearest {
    const UNSET: Self = Self {
        origin: Vec3 {
            x: f64::MAX,
            y: f64::MAX,
            z: f64::MAX,
        },
        dist2: -f64::MAX,
    };

    fn valid(&self) -> bool {
        self.dist2 > -SMALL
    }

    /// `wallPoint::update` with `FaceCellWave`'s 1 % tolerance: takes the
    /// other origin when it is clearly nearer to `at`.
    fn update(&mut self, at: Vec3, other: Nearest) -> bool {
        let d = at - other.origin;
        let dist2 = d.x * d.x + d.y * d.y + d.z * d.z;
        if !self.valid() {
            *self = Self {
                origin: other.origin,
                dist2,
            };
            return true;
        }
        let diff = self.dist2 - dist2;
        if diff < 0.0 || diff < SMALL || (self.dist2 > SMALL && diff / self.dist2 < 0.01) {
            return false;
        }
        *self = Self {
            origin: other.origin,
            dist2,
        };
        true
    }
}

/// Derived from OpenFOAM's `patchWave`, `FaceCellWave`, `wallPoint` and
/// `cellDistFuncs` (OpenFOAM, GPL-3.0-or-later). Each wall face's centre
/// spreads through the mesh, face to cell to face, each cell or face keeping
/// the nearest centre it has been offered, in OpenFOAM's order; then the
/// cells touching a wall are given their true distance to the nearest point
/// of the wall faces around them. Boundary faces keep the distance to the
/// centre their cell passed on, plus `SMALL`.
pub fn wall_distance(
    mesh: &PolyMesh,
    cell_centres: &[Vec3],
    face_centres: &[Vec3],
) -> WallDistance {
    let n_internal = mesh.n_internal_faces();
    let owner = mesh.owner();
    let neighbour = mesh.neighbour();
    let walls: Vec<&Patch> = mesh
        .patches()
        .iter()
        .filter(|p| p.kind == PatchKind::Wall)
        .collect();

    // A cell's faces as `primitiveMesh::cells` lists them: those it owns,
    // then those it neighbours, each in face order.
    let mut cell_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.n_cells()];
    for (f, &c) in owner.iter().enumerate() {
        if let Some(list) = cell_faces.get_mut(c) {
            list.push(f);
        }
    }
    for (f, &c) in neighbour.iter().enumerate() {
        if let Some(list) = cell_faces.get_mut(c) {
            list.push(f);
        }
    }

    let mut cells = vec![Nearest::UNSET; mesh.n_cells()];
    let mut faces = vec![Nearest::UNSET; mesh.n_faces()];
    let mut changed_faces = Vec::new();
    let mut face_flag = vec![false; mesh.n_faces()];
    let mut cell_flag = vec![false; mesh.n_cells()];
    for patch in &walls {
        for f in patch.faces() {
            let (Some(slot), Some(&origin)) = (faces.get_mut(f), face_centres.get(f)) else {
                continue;
            };
            *slot = Nearest { origin, dist2: 0.0 };
            if let Some(flag) = face_flag.get_mut(f) {
                *flag = true;
            }
            changed_faces.push(f);
        }
    }

    let same = |a: &Nearest, b: &Nearest| a.origin == b.origin;
    let mut changed_cells = Vec::new();
    for _ in 0..=mesh.n_cells() {
        for &f in &changed_faces {
            let Some(&info) = faces.get(f) else { continue };
            let sides = [
                owner.get(f).copied(),
                if f < n_internal {
                    neighbour.get(f).copied()
                } else {
                    None
                },
            ];
            for c in sides.into_iter().flatten() {
                let (Some(cell), Some(&at)) = (cells.get_mut(c), cell_centres.get(c)) else {
                    continue;
                };
                if !same(cell, &info)
                    && cell.update(at, info)
                    && let Some(flag) = cell_flag.get_mut(c)
                    && !*flag
                {
                    *flag = true;
                    changed_cells.push(c);
                }
            }
            if let Some(flag) = face_flag.get_mut(f) {
                *flag = false;
            }
        }
        changed_faces.clear();
        if changed_cells.is_empty() {
            break;
        }
        for &c in &changed_cells {
            let Some(&info) = cells.get(c) else { continue };
            for &f in cell_faces.get(c).map(Vec::as_slice).unwrap_or(&[]) {
                let (Some(face), Some(&at)) = (faces.get_mut(f), face_centres.get(f)) else {
                    continue;
                };
                if !same(face, &info)
                    && face.update(at, info)
                    && let Some(flag) = face_flag.get_mut(f)
                    && !*flag
                {
                    *flag = true;
                    changed_faces.push(f);
                }
            }
            if let Some(flag) = cell_flag.get_mut(c) {
                *flag = false;
            }
        }
        changed_cells.clear();
        if changed_faces.is_empty() {
            break;
        }
    }

    let distance = |n: &Nearest| if n.valid() { n.dist2.sqrt() } else { n.dist2 };
    let mut cell_distance: Vec<f64> = cells.iter().map(distance).collect();
    let mut face_distance_out = Vec::with_capacity(mesh.n_faces().saturating_sub(n_internal));
    for patch in mesh.patches() {
        for f in patch.faces() {
            face_distance_out.push(if patch.kind == PatchKind::Empty {
                None
            } else {
                faces.get(f).map(|n| {
                    if n.valid() {
                        n.dist2.sqrt() + SMALL
                    } else {
                        n.dist2
                    }
                })
            });
        }
    }

    // The cells with a face on a wall, patch by patch, a later patch
    // overwriting an earlier one, as `correctBoundaryFaceCells` does.
    let mut corrected = vec![false; mesh.n_cells()];
    for patch in &walls {
        let near = near_wall_distance(mesh, patch, cell_centres);
        for (f, d) in patch.faces().zip(near) {
            let Some(&c) = owner.get(f) else { continue };
            if let Some(v) = cell_distance.get_mut(c) {
                *v = d;
            }
            if let Some(flag) = corrected.get_mut(c) {
                *flag = true;
            }
        }
    }
    // Then the cells touching a wall at a point only, from the faces around
    // the first such point, in the order the patch's points first appear.
    let mut point_cells: HashMap<usize, Vec<usize>> = HashMap::new();
    for patch in &walls {
        for f in patch.faces() {
            for &p in mesh.face(f) {
                point_cells.entry(p).or_default();
            }
        }
    }
    for (c, list) in cell_faces.iter().enumerate() {
        for &f in list {
            for &p in mesh.face(f) {
                if let Some(cs) = point_cells.get_mut(&p)
                    && cs.last() != Some(&c)
                {
                    cs.push(c);
                }
            }
        }
    }
    let mut pts: Vec<Vec3> = Vec::new();
    for patch in &walls {
        let mut point_faces: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut order = Vec::new();
        for f in patch.faces() {
            for &p in mesh.face(f) {
                let list = point_faces.entry(p).or_default();
                if list.is_empty() {
                    order.push(p);
                }
                list.push(f);
            }
        }
        for p in order {
            let around = point_faces.get(&p).map(Vec::as_slice).unwrap_or(&[]);
            for &c in point_cells.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
                if corrected.get(c).copied().unwrap_or(true) {
                    continue;
                }
                let centre = cell_centres.get(c).copied().unwrap_or_default();
                let mut smallest = f64::MAX;
                for &g in around {
                    pts.clear();
                    pts.extend(mesh.face_points(g));
                    let d = face_distance(&pts, centre);
                    if d < smallest {
                        smallest = d;
                    }
                }
                if let Some(v) = cell_distance.get_mut(c) {
                    *v = smallest;
                }
                if let Some(flag) = corrected.get_mut(c) {
                    *flag = true;
                }
            }
        }
    }
    WallDistance {
        cells: cell_distance,
        faces: face_distance_out,
    }
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

    #[test]
    fn a_channel_measures_cells_to_the_nearer_wall_and_inlet_faces_to_their_cells_wall_face() {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 0.1, 0.01),
        };
        let mut raw = crate::blocks::structured_box_raw(bounds, [40, 10, 1]).unwrap();
        for p in &mut raw.patches {
            if p.name.starts_with('y') {
                p.kind = "wall".into();
            }
            if p.name.starts_with('z') {
                p.kind = "empty".into();
            }
        }
        let mesh = PolyMesh::from_raw(raw).unwrap();
        let geo = MeshGeometry::compute(&mesh);
        let y = wall_distance(&mesh, &geo.cell_centres, &geo.face_centres);
        for (c, centre) in geo.cell_centres.iter().enumerate() {
            let exact = centre.y.min(0.1 - centre.y);
            assert!(
                (y.cells[c] - exact).abs() < 1e-15,
                "cell {c}: {}",
                y.cells[c]
            );
        }
        let n_internal = mesh.n_internal_faces();
        let x_min = mesh.patch("xMin").unwrap();
        for f in x_min.faces() {
            let yc = geo.face_centres[f].y.min(0.1 - geo.face_centres[f].y);
            let expected = (0.0125f64 * 0.0125 + yc * yc).sqrt() + 1e-15;
            let got = y.faces[f - n_internal].unwrap();
            assert!((got - expected).abs() < 1e-15, "face {f}: {got}");
        }
        for f in mesh.patch("yMin").unwrap().faces() {
            assert_eq!(y.faces[f - n_internal], Some(1e-15));
        }
        assert!(
            mesh.patch("zMin")
                .unwrap()
                .faces()
                .all(|f| y.faces[f - n_internal].is_none())
        );
    }
}
