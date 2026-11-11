//! Mesh quality: the questions OpenFOAM's `checkMesh` asks, answered from
//! this engine's own geometry.
//!
//! The measures and thresholds follow OpenFOAM's `primitiveMesh` checks
//! (`checkClosedCells`, `checkFaceOrthogonality`, `checkFaceSkewness`,
//! `checkFacePyramids`, `checkUpperTriangular`; OpenFOAM, GPL-3.0-or-later),
//! so a report can be read side by side with `checkMesh`'s log.

use vortia_core::{Aabb, Vec3};

use crate::geometry::MeshGeometry;
use crate::polymesh::PolyMesh;

const VSMALL: f64 = 1.0e-300;
const ROOTVSMALL: f64 = 1.0e-150;

/// A cell whose face area vectors do not sum to zero within this fraction of
/// their total is open.
pub const CLOSED_THRESHOLD: f64 = 1.0e-6;
/// Non-orthogonality, in degrees, above which a face is reported as severe.
pub const NON_ORTH_THRESHOLD: f64 = 70.0;
/// Skewness above which a face is reported as highly skew.
pub const SKEW_THRESHOLD: f64 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl Stat {
    fn of(values: impl Iterator<Item = f64>) -> Option<Self> {
        let (mut min, mut max, mut sum, mut n) = (f64::INFINITY, f64::NEG_INFINITY, 0.0, 0usize);
        for v in values {
            min = min.min(v);
            max = max.max(v);
            sum += v;
            n += 1;
        }
        (n > 0).then(|| Self {
            min,
            max,
            mean: sum / n as f64,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshReport {
    pub n_points: usize,
    pub n_faces: usize,
    pub n_internal_faces: usize,
    pub n_cells: usize,
    pub bounds: Aabb,
    pub total_volume: f64,
    pub volume: Option<Stat>,
    pub face_area: Option<Stat>,
    /// Internal faces only, in degrees.
    pub non_orthogonality: Option<Stat>,
    pub skewness: Option<Stat>,
    pub unused_points: usize,
    pub open_cells: usize,
    pub non_positive_volumes: usize,
    pub zero_area_faces: usize,
    pub severely_non_orthogonal: usize,
    pub highly_skew: usize,
    /// Faces whose normal points into their owner or out of their neighbour.
    pub wrongly_oriented: usize,
    /// Internal faces not in upper-triangular order (owner ascending, then
    /// neighbour ascending within each owner), which the solver relies on.
    pub upper_triangular: bool,
}

impl MeshReport {
    /// Problems that make the mesh unusable for a solve, as OpenFOAM's
    /// `checkMesh` counts them ("Failed N mesh checks").
    pub fn failed_checks(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut fail = |n: usize, what: &str| {
            if n > 0 {
                out.push(format!("{n} {what}"));
            }
        };
        fail(self.open_cells, "open cells");
        fail(
            self.non_positive_volumes,
            "cells with zero or negative volume",
        );
        fail(self.zero_area_faces, "faces with zero area");
        fail(self.wrongly_oriented, "wrongly oriented faces");
        fail(
            self.severely_non_orthogonal,
            "severely non-orthogonal faces",
        );
        fail(self.highly_skew, "highly skew faces");
        if !self.upper_triangular {
            out.push("internal faces not in upper-triangular order".into());
        }
        out
    }

    pub fn is_ok(&self) -> bool {
        self.failed_checks().is_empty()
    }
}

pub fn check(mesh: &PolyMesh, geo: &MeshGeometry) -> MeshReport {
    let own = mesh.owner();
    let nei = mesh.neighbour();
    let n_internal = mesh.n_internal_faces();

    let mut bounds = Aabb::default();
    let mut first = true;
    for &p in mesh.points() {
        if first {
            bounds = Aabb { min: p, max: p };
            first = false;
        } else {
            bounds.extend(p);
        }
    }

    let mut used = vec![false; mesh.n_points()];
    for &p in &mesh.faces().points {
        if let Some(u) = used.get_mut(p) {
            *u = true;
        }
    }

    // Closedness: the area vectors of a closed cell sum to zero.
    let mut sum_closed = vec![Vec3::default(); mesh.n_cells()];
    let mut sum_mag = vec![Vec3::default(); mesh.n_cells()];
    let abs = |v: Vec3| Vec3::new(v.x.abs(), v.y.abs(), v.z.abs());
    for (facei, &a) in geo.face_areas.iter().enumerate() {
        if let Some(&o) = own.get(facei)
            && let (Some(s), Some(m)) = (sum_closed.get_mut(o), sum_mag.get_mut(o))
        {
            *s = *s + a;
            *m = *m + abs(a);
        }
        if let Some(&n) = nei.get(facei)
            && let (Some(s), Some(m)) = (sum_closed.get_mut(n), sum_mag.get_mut(n))
        {
            *s = *s - a;
            *m = *m + abs(a);
        }
    }
    let open_cells = sum_closed
        .iter()
        .zip(&sum_mag)
        .filter(|(s, m)| {
            let ratio = |s: f64, m: f64| if m > VSMALL { s.abs() / m } else { 0.0 };
            ratio(s.x, m.x).max(ratio(s.y, m.y)).max(ratio(s.z, m.z)) > CLOSED_THRESHOLD
        })
        .count();

    let mut non_orth = Vec::with_capacity(n_internal);
    let mut skew = Vec::with_capacity(mesh.n_faces());
    let mut wrongly_oriented = 0;
    for facei in 0..mesh.n_faces() {
        let (Some(&sf), Some(&fc), Some(&o)) = (
            geo.face_areas.get(facei),
            geo.face_centres.get(facei),
            own.get(facei),
        ) else {
            continue;
        };
        let Some(&c_own) = geo.cell_centres.get(o) else {
            continue;
        };
        if sf.dot(fc - c_own) <= 0.0 {
            wrongly_oriented += 1;
        }
        let neighbour_centre = nei
            .get(facei)
            .and_then(|&n| geo.cell_centres.get(n))
            .copied();
        if let Some(c_nei) = neighbour_centre {
            if sf.dot(c_nei - fc) <= 0.0 {
                wrongly_oriented += 1;
            }
            let d = c_nei - c_own;
            let cos = d.dot(sf) / (d.norm() * sf.norm() + VSMALL);
            non_orth.push(cos.clamp(-1.0, 1.0).acos().to_degrees());
        }
        skew.push(face_skewness(mesh, facei, sf, fc, c_own, neighbour_centre));
    }

    let mut upper_triangular = true;
    for (pair_o, pair_n) in own.windows(2).zip(nei.windows(2)) {
        if let ([o0, o1], [n0, n1]) = (pair_o, pair_n)
            && (o1 < o0 || (o0 == o1 && n1 <= n0))
        {
            upper_triangular = false;
            break;
        }
    }
    if own.iter().zip(nei).any(|(o, n)| o >= n) {
        upper_triangular = false;
    }

    MeshReport {
        n_points: mesh.n_points(),
        n_faces: mesh.n_faces(),
        n_internal_faces: n_internal,
        n_cells: mesh.n_cells(),
        bounds,
        total_volume: geo.total_volume(),
        volume: Stat::of(geo.cell_volumes.iter().copied()),
        face_area: Stat::of(geo.face_areas.iter().map(|a| a.norm())),
        unused_points: used.iter().filter(|u| !**u).count(),
        open_cells,
        non_positive_volumes: geo.cell_volumes.iter().filter(|&&v| v <= VSMALL).count(),
        zero_area_faces: geo.face_areas.iter().filter(|a| a.norm() <= VSMALL).count(),
        severely_non_orthogonal: non_orth.iter().filter(|&&a| a > NON_ORTH_THRESHOLD).count(),
        highly_skew: skew.iter().filter(|&&s| s > SKEW_THRESHOLD).count(),
        non_orthogonality: Stat::of(non_orth.into_iter()),
        skewness: Stat::of(skew.into_iter()),
        wrongly_oriented,
        upper_triangular,
    }
}

/// How far the face centre lies from where the line between the two cell
/// centres crosses the face, relative to the cell spacing and the face size.
fn face_skewness(
    mesh: &PolyMesh,
    facei: usize,
    sf: Vec3,
    fc: Vec3,
    c_own: Vec3,
    c_nei: Option<Vec3>,
) -> f64 {
    let cpf = fc - c_own;
    let (d, scale) = match c_nei {
        Some(c_nei) => (c_nei - c_own, 0.2),
        None => {
            // A boundary face mirrors the owner centre through the face.
            let n = sf / (sf.norm() + VSMALL);
            (n * n.dot(cpf), 0.4)
        }
    };
    let sv = cpf - d * (sf.dot(cpf) / (sf.dot(d) + ROOTVSMALL));
    let sv_hat = sv / (sv.norm() + VSMALL);
    let mut fd = scale * d.norm() + ROOTVSMALL;
    for p in mesh.face_points(facei) {
        fd = fd.max(sv_hat.dot(p - fc).abs());
    }
    sv.norm() / fd
}
