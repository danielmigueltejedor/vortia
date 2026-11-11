//! Face and cell geometry: centres, area vectors and volumes.
//!
//! Derived from OpenFOAM's `primitiveMesh::makeFaceCentresAndAreas` and
//! `primitiveMesh::makeCellCentresAndVols` (OpenFOAM, Copyright (C) 2011-2016
//! OpenFOAM Foundation and OpenCFD Ltd., GPL-3.0-or-later). Using the same
//! decomposition matters beyond style: it is what makes this engine's cell
//! volumes and centres agree with OpenFOAM's to rounding, so results can be
//! compared cell by cell.
//!
//! A face is split into triangles around the average of its points; its centre
//! is the area-weighted centre of those triangles and its area vector their
//! sum, which stays correct for non-planar faces. A cell is split into
//! pyramids, one per face, with the apex at the average of its face centres.

use vortia_core::Vec3;

use crate::polymesh::PolyMesh;

const VSMALL: f64 = 1.0e-300;
const ROOTVSMALL: f64 = 1.0e-150;

#[derive(Debug, Clone, PartialEq)]
pub struct MeshGeometry {
    pub face_centres: Vec<Vec3>,
    /// Area vectors: normal to the face, pointing out of the owner, with the
    /// face's area as length.
    pub face_areas: Vec<Vec3>,
    pub cell_centres: Vec<Vec3>,
    pub cell_volumes: Vec<f64>,
}

impl MeshGeometry {
    pub fn compute(mesh: &PolyMesh) -> Self {
        let (face_centres, face_areas) = face_centres_and_areas(mesh);
        let (cell_centres, cell_volumes) =
            cell_centres_and_volumes(mesh, &face_centres, &face_areas);
        Self {
            face_centres,
            face_areas,
            cell_centres,
            cell_volumes,
        }
    }

    pub fn total_volume(&self) -> f64 {
        self.cell_volumes.iter().sum()
    }
}

fn face_centres_and_areas(mesh: &PolyMesh) -> (Vec<Vec3>, Vec<Vec3>) {
    let n = mesh.n_faces();
    let mut centres = Vec::with_capacity(n);
    let mut areas = Vec::with_capacity(n);
    let mut pts: Vec<Vec3> = Vec::new();
    for facei in 0..n {
        pts.clear();
        pts.extend(mesh.face_points(facei));
        let (c, a) = face_centre_and_area(&pts);
        centres.push(c);
        areas.push(a);
    }
    (centres, areas)
}

/// The centre and area vector of one face given its points in order.
pub fn face_centre_and_area(pts: &[Vec3]) -> (Vec3, Vec3) {
    match pts {
        [] | [_] | [_, _] => (mean(pts), Vec3::default()),
        [p0, p1, p2] => ((*p0 + *p1 + *p2) / 3.0, (*p1 - *p0).cross(*p2 - *p0) * 0.5),
        _ => {
            let estimate = mean(pts);
            let mut sum_n = Vec3::default();
            let mut sum_a = 0.0;
            let mut sum_ac = Vec3::default();
            for (i, &p) in pts.iter().enumerate() {
                let next = pts.get(i + 1).or(pts.first()).copied().unwrap_or(p);
                let c = p + next + estimate;
                let n = (next - p).cross(estimate - p);
                let a = n.norm();
                sum_n = sum_n + n;
                sum_a += a;
                sum_ac = sum_ac + c * a;
            }
            if sum_a < ROOTVSMALL {
                (estimate, Vec3::default())
            } else {
                (sum_ac / (3.0 * sum_a), sum_n * 0.5)
            }
        }
    }
}

fn mean(pts: &[Vec3]) -> Vec3 {
    if pts.is_empty() {
        return Vec3::default();
    }
    pts.iter().fold(Vec3::default(), |s, &p| s + p) / pts.len() as f64
}

fn cell_centres_and_volumes(mesh: &PolyMesh, fc: &[Vec3], fa: &[Vec3]) -> (Vec<Vec3>, Vec<f64>) {
    let n_cells = mesh.n_cells();
    let own = mesh.owner();
    let nei = mesh.neighbour();

    // The apex of each cell's pyramids: the mean of its face centres.
    let mut estimate = vec![Vec3::default(); n_cells];
    let mut n_faces = vec![0u32; n_cells];
    let mut add = |cell: usize, c: Vec3| {
        if let (Some(e), Some(k)) = (estimate.get_mut(cell), n_faces.get_mut(cell)) {
            *e = *e + c;
            *k += 1;
        }
    };
    for (&c, &o) in fc.iter().zip(own) {
        add(o, c);
    }
    for (&c, &n) in fc.iter().zip(nei) {
        add(n, c);
    }
    for (e, &k) in estimate.iter_mut().zip(&n_faces) {
        if k > 0 {
            *e = *e / f64::from(k);
        }
    }

    let mut centres = vec![Vec3::default(); n_cells];
    let mut volumes = vec![0.0; n_cells];
    let mut pyramid = |cell: usize, c: Vec3, pyr3_vol: f64| {
        let Some(&apex) = estimate.get(cell) else {
            return;
        };
        if let (Some(cc), Some(v)) = (centres.get_mut(cell), volumes.get_mut(cell)) {
            // The centroid of a pyramid is 3/4 of the way from apex to base.
            *cc = *cc + (c * 0.75 + apex * 0.25) * pyr3_vol;
            *v += pyr3_vol;
        }
    };
    for (facei, (&c, &a)) in fc.iter().zip(fa).enumerate() {
        if let Some(&o) = own.get(facei) {
            let apex = estimate.get(o).copied().unwrap_or_default();
            pyramid(o, c, a.dot(c - apex));
        }
        if let Some(&n) = nei.get(facei) {
            let apex = estimate.get(n).copied().unwrap_or_default();
            pyramid(n, c, a.dot(apex - c));
        }
    }

    for ((cc, v), &e) in centres.iter_mut().zip(volumes.iter_mut()).zip(&estimate) {
        *cc = if v.abs() > VSMALL { *cc / *v } else { e };
        *v /= 3.0;
    }
    (centres, volumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_face_has_its_centre_and_unit_normal() {
        let pts = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let (c, a) = face_centre_and_area(&pts);
        assert!((c - Vec3::new(0.5, 1.0, 0.0)).norm() < 1e-15);
        assert!((a - Vec3::new(0.0, 0.0, 2.0)).norm() < 1e-15);
    }

    #[test]
    fn a_degenerate_face_has_no_area_and_no_nan() {
        let p = Vec3::new(1.0, 1.0, 1.0);
        let (c, a) = face_centre_and_area(&[p, p, p, p]);
        assert_eq!((c, a), (p, Vec3::default()));
        assert_eq!(face_centre_and_area(&[]).1, Vec3::default());
    }
}
