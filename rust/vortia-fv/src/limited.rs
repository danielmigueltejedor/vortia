//! Gradient schemes beyond `Gauss linear`: `cellLimited Gauss linear k`,
//! and the `linearUpwind` face correction that reads them.
//!
//! Derived from OpenFOAM's `cellLimitedGrad` (with the `minmod` limiter),
//! `gaussGrad` and `linearUpwind` (OpenFOAM, Copyright (C) 2011-2017 OpenFOAM
//! Foundation and OpenCFD Ltd., GPL-3.0-or-later).

use vortia_core::Vec3;
use vortia_mesh::PolyMesh;

use crate::error::{FvError, Result};
use crate::fvgeom::FvGeometry;

/// OpenFOAM's `SMALL`.
const SMALL: f64 = 1e-15;

/// A `gradSchemes` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradScheme {
    /// `Gauss linear`.
    GaussLinear,
    /// `cellLimited Gauss linear k`: the Gauss gradient, scaled in each cell
    /// so that the value it extrapolates to a face stays between the
    /// neighbours' values (widened by `1/k − 1`).
    CellLimited(f64),
}

impl GradScheme {
    /// From a `gradSchemes` entry such as `cellLimited Gauss linear 1`.
    pub fn from_scheme(words: &[&str]) -> Result<Self> {
        match words {
            ["Gauss", "linear"] => Ok(Self::GaussLinear),
            ["cellLimited", "Gauss", "linear", k] => {
                let k: f64 = k.parse().map_err(|_| {
                    FvError::Setup(format!("grad scheme limit '{k}' is not a number"))
                })?;
                if !(0.0..=1.0).contains(&k) {
                    return Err(FvError::Setup(format!(
                        "grad scheme limit {k} is outside 0 to 1"
                    )));
                }
                Ok(Self::CellLimited(k))
            }
            other => Err(FvError::Setup(format!(
                "grad scheme '{}' is not supported yet; use Gauss linear or \
                 cellLimited Gauss linear 1",
                other.join(" ")
            ))),
        }
    }
}

/// The Gauss gradient of a cell field with the given values on the boundary
/// faces (`None` where there are none).
pub fn gauss_grad(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[f64],
    boundary: &[Option<f64>],
) -> Vec<Vec3> {
    let mut g = vec![Vec3::default(); mesh.n_cells()];
    let sf = &fv.geo.face_areas;
    let own = mesh.owner();
    for (f, (&o, &nb)) in own.iter().zip(mesh.neighbour()).enumerate() {
        let (Some(&w), Some(&s), Some(&po), Some(&pn)) =
            (fv.weights.get(f), sf.get(f), cells.get(o), cells.get(nb))
        else {
            continue;
        };
        // `w·P + (1 − w)·N`, the form OpenFOAM v1912's gradient uses: the
        // algebraically equal `w·(P − N) + N` changes solver iteration counts.
        let face = s * (w * po + (1.0 - w) * pn);
        if let Some(go) = g.get_mut(o) {
            *go = *go + face;
        }
        if let Some(gn) = g.get_mut(nb) {
            *gn = *gn - face;
        }
    }
    for (k, v) in boundary.iter().enumerate() {
        let f = mesh.n_internal_faces() + k;
        let (Some(v), Some(&s), Some(&o)) = (v, sf.get(f), own.get(f)) else {
            continue;
        };
        if let Some(go) = g.get_mut(o) {
            *go = *go + s * *v;
        }
    }
    for (gc, &vol) in g.iter_mut().zip(fv.volumes()) {
        if vol > 0.0 {
            *gc = *gc / vol;
        }
    }
    g
}

/// The scale `cellLimited` gives each cell's gradient `g` of `cells`, with
/// `minmod` as the limiter: 1 where the extrapolated values stay inside
/// the range of the cell's neighbours, less where they do not.
pub fn cell_limiter(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[f64],
    boundary: &[Option<f64>],
    g: &[Vec3],
    k: f64,
) -> Vec<f64> {
    let n = cells.len();
    if k < SMALL {
        return vec![1.0; n];
    }
    let own = mesh.owner();
    let mut max_v = cells.to_vec();
    let mut min_v = cells.to_vec();
    let widen = |a: &mut Vec<f64>, b: &mut Vec<f64>, c: usize, v: f64| {
        if let (Some(x), Some(y)) = (a.get_mut(c), b.get_mut(c)) {
            *x = x.max(v);
            *y = y.min(v);
        }
    };
    for (&o, &nb) in own.iter().zip(mesh.neighbour()) {
        let (Some(&vo), Some(&vn)) = (cells.get(o), cells.get(nb)) else {
            continue;
        };
        widen(&mut max_v, &mut min_v, o, vn);
        widen(&mut max_v, &mut min_v, nb, vo);
    }
    let n_internal = mesh.n_internal_faces();
    for (i, v) in boundary.iter().enumerate() {
        if let (Some(v), Some(&o)) = (v, own.get(n_internal + i)) {
            widen(&mut max_v, &mut min_v, o, *v);
        }
    }
    for ((hi, lo), &c) in max_v.iter_mut().zip(&mut min_v).zip(cells) {
        *hi -= c;
        *lo -= c;
    }
    if k < 1.0 {
        for (hi, lo) in max_v.iter_mut().zip(&mut min_v) {
            let extra = (1.0 / k - 1.0) * (*hi - *lo);
            *hi += extra;
            *lo -= extra;
        }
    }

    let centres = &fv.geo.cell_centres;
    let cf = &fv.geo.face_centres;
    let mut limiter = vec![1.0_f64; n];
    let mut limit = |c: usize, face: usize| {
        let (Some(&centre), Some(&face_centre), Some(&grad), Some(&hi), Some(&lo)) = (
            centres.get(c),
            cf.get(face),
            g.get(c),
            max_v.get(c),
            min_v.get(c),
        ) else {
            return;
        };
        let extrapolate = (face_centre - centre).dot(grad);
        let r = if extrapolate > SMALL {
            hi / extrapolate
        } else if extrapolate < -SMALL {
            lo / extrapolate
        } else {
            return;
        };
        if let Some(l) = limiter.get_mut(c) {
            *l = (*l).min(r.min(1.0));
        }
    };
    for (f, (&o, &nb)) in own.iter().zip(mesh.neighbour()).enumerate() {
        limit(o, f);
        limit(nb, f);
    }
    for (i, v) in boundary.iter().enumerate() {
        let f = n_internal + i;
        if let (Some(_), Some(&o)) = (v, own.get(f)) {
            limit(o, f);
        }
    }
    limiter
}

/// The gradient of a cell field under `scheme`, given its values on the
/// boundary faces.
pub fn grad(
    scheme: GradScheme,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[f64],
    boundary: &[Option<f64>],
) -> Vec<Vec3> {
    let mut g = gauss_grad(mesh, fv, cells, boundary);
    if let GradScheme::CellLimited(k) = scheme {
        let limiter = cell_limiter(mesh, fv, cells, boundary, &g, k);
        for (gc, l) in g.iter_mut().zip(limiter) {
            *gc = *gc * l;
        }
    }
    g
}

/// `linearUpwind`'s correction of the upwind face value, on every internal
/// face: `(C_f − C_up) · ∇ψ_up`, with the gradient under `scheme` and the
/// upwind cell the one the flux `phi` leaves (the neighbour when it is zero).
pub fn linear_upwind_correction(
    scheme: GradScheme,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    phi: &[f64],
    cells: &[f64],
    boundary: &[Option<f64>],
) -> Vec<f64> {
    let g = grad(scheme, mesh, fv, cells, boundary);
    correction_from(mesh, fv, phi, &g)
}

/// The same from a gradient already in hand.
pub fn correction_from(mesh: &PolyMesh, fv: &FvGeometry, phi: &[f64], g: &[Vec3]) -> Vec<f64> {
    let centres = &fv.geo.cell_centres;
    let cf = &fv.geo.face_centres;
    mesh.owner()
        .iter()
        .zip(mesh.neighbour())
        .enumerate()
        .map(|(f, (&o, &nb))| {
            let up = if phi.get(f).copied().unwrap_or(0.0) > 0.0 {
                o
            } else {
                nb
            };
            match (centres.get(up), cf.get(f), g.get(up)) {
                (Some(&c), Some(&face), Some(&grad)) => (face - c).dot(grad),
                _ => 0.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box;

    use super::*;

    /// Four cells in a row along x, unit cross-section.
    fn row() -> (PolyMesh, FvGeometry) {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(4.0, 1.0, 1.0),
        };
        let mesh = structured_box(bounds, [4, 1, 1]).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    /// Boundary faces in patch order, with the value `f` of their centre.
    fn edge(mesh: &PolyMesh, fv: &FvGeometry, f: impl Fn(Vec3) -> f64) -> Vec<Option<f64>> {
        (mesh.n_internal_faces()..mesh.n_faces())
            .map(|face| fv.geo.face_centres.get(face).map(|&c| f(c)))
            .collect()
    }

    #[test]
    fn grad_schemes_are_read_with_their_limit() {
        use GradScheme::*;
        assert_eq!(
            GradScheme::from_scheme(&["Gauss", "linear"]).unwrap(),
            GaussLinear
        );
        assert_eq!(
            GradScheme::from_scheme(&["cellLimited", "Gauss", "linear", "1"]).unwrap(),
            CellLimited(1.0)
        );
        for bad in [
            vec!["cellLimited", "Gauss", "linear"],
            vec!["cellLimited", "Gauss", "linear", "2"],
            vec!["cellLimited", "Gauss", "linear", "x"],
            vec!["leastSquares"],
        ] {
            assert!(GradScheme::from_scheme(&bad).is_err(), "{bad:?}");
        }
    }

    /// A field that is linear in space has the same gradient everywhere, and
    /// the limiter has nothing to limit: its extrapolated face values are the
    /// neighbours' values at most.
    #[test]
    fn a_linear_field_keeps_its_gradient_under_the_limiter() {
        let (mesh, fv) = row();
        let value = |c: Vec3| 2.0 * c.x + 1.0;
        let cells: Vec<f64> = fv.geo.cell_centres.iter().map(|&c| value(c)).collect();
        let boundary = edge(&mesh, &fv, value);
        let plain = grad(GradScheme::GaussLinear, &mesh, &fv, &cells, &boundary);
        let limited = grad(GradScheme::CellLimited(1.0), &mesh, &fv, &cells, &boundary);
        for (a, b) in plain.iter().zip(&limited) {
            assert!((a.x - 2.0).abs() < 1e-12, "{a:?}");
            assert!((b.x - 2.0).abs() < 1e-12, "{b:?}");
        }
    }

    /// At a peak, the Gauss gradient would push the face values past the
    /// neighbours'; the limiter scales it back until they just reach them.
    /// Every cell here also touches boundary faces of value 0.
    #[test]
    fn a_peak_is_limited_to_the_range_of_its_neighbours() {
        let (mesh, fv) = row();
        let cells = [0.0, 10.0, 1.0, 0.0];
        let boundary = edge(&mesh, &fv, |_| 0.0);
        let plain = gauss_grad(&mesh, &fv, &cells, &boundary);
        let scaled = grad(GradScheme::CellLimited(1.0), &mesh, &fv, &cells, &boundary);
        let mut limited_somewhere = false;
        for (c, (p, s)) in plain.iter().zip(&scaled).enumerate() {
            let around = [c.checked_sub(1), Some(c + 1)]
                .into_iter()
                .flatten()
                .filter_map(|n| cells.get(n).copied())
                .chain([0.0]);
            let own = cells.get(c).copied().unwrap_or(0.0);
            let (lo, hi) = around.fold((own, own), |(lo, hi), v| (lo.min(v), hi.max(v)));
            // Half a cell along x reaches the face the gradient points to.
            let reach = 0.5 * s.x.abs();
            assert!(own + reach <= hi + 1e-12, "cell {c} overshoots {hi}");
            assert!(own - reach >= lo - 1e-12, "cell {c} undershoots {lo}");
            assert!(s.x.abs() <= p.x.abs() + 1e-12, "cell {c} grew");
            limited_somewhere |= (s.x - p.x).abs() > 1e-9;
        }
        assert!(limited_somewhere, "the peak was not limited");
    }

    /// The correction carries the upwind cell's gradient to the face: exact
    /// for a linear field, from whichever side the flux comes.
    #[test]
    fn linear_upwind_reaches_the_face_value_of_a_linear_field() {
        let (mesh, fv) = row();
        let value = |c: Vec3| 3.0 * c.x - 1.0;
        let cells: Vec<f64> = fv.geo.cell_centres.iter().map(|&c| value(c)).collect();
        let boundary = edge(&mesh, &fv, value);
        for sign in [1.0, -1.0] {
            let phi = vec![sign; mesh.n_faces()];
            let corr = linear_upwind_correction(
                GradScheme::GaussLinear,
                &mesh,
                &fv,
                &phi,
                &cells,
                &boundary,
            );
            assert_eq!(corr.len(), mesh.n_internal_faces());
            for (f, c) in corr.iter().enumerate() {
                let (Some(&o), Some(&n)) = (mesh.owner().get(f), mesh.neighbour().get(f)) else {
                    continue;
                };
                let up = if sign > 0.0 { o } else { n };
                let face = fv.geo.face_centres[f];
                let upwind = cells[up];
                assert!(
                    (upwind + c - value(face)).abs() < 1e-12,
                    "face {f}: {upwind} + {c}"
                );
            }
        }
    }
}
