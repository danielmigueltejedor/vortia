//! The explicit non-orthogonal correction of `Gauss linear corrected`:
//! `correctedSnGrad`'s `nonOrthCorrectionVectors & interpolate(∇ψ)` on the
//! internal faces, and what it does to a Laplacian.
//!
//! Derived from OpenFOAM's `correctedSnGrad::fullGradCorrection` and
//! `gaussLaplacianScheme::fvmLaplacian` (OpenFOAM, Copyright (C) 2011-2017
//! OpenFOAM Foundation and OpenCFD Ltd., GPL-3.0-or-later). Boundary faces
//! have no correction.

use vortia_core::Vec3;
use vortia_mesh::PolyMesh;

use crate::fvgeom::FvGeometry;
use crate::ops::SnGrad;
use crate::stress::{Tensor, dot};
use crate::vector::{VectorBc, VectorBoundary};

/// `corrVec · (w ∇ψ_P + (1 − w) ∇ψ_N)` on every internal face, from the cell
/// gradients `g` of a scalar.
pub fn sn_grad_correction(mesh: &PolyMesh, fv: &FvGeometry, g: &[Vec3]) -> Vec<f64> {
    mesh.owner()
        .iter()
        .zip(mesh.neighbour())
        .enumerate()
        .map(
            |(f, (&o, &n))| match (fv.corr_vecs.get(f), fv.weights.get(f), g.get(o), g.get(n)) {
                (Some(&cv), Some(&w), Some(&p), Some(&q)) => cv.dot((p - q) * w + q),
                _ => 0.0,
            },
        )
        .collect()
}

/// The same for a vector: `corrVec · (w ∇u_P + (1 − w) ∇u_N)`, from the
/// cells' gradient tensors.
pub fn sn_grad_correction_vector(mesh: &PolyMesh, fv: &FvGeometry, g: &[Tensor]) -> Vec<Vec3> {
    mesh.owner()
        .iter()
        .zip(mesh.neighbour())
        .enumerate()
        .map(
            |(f, (&o, &n))| match (fv.corr_vecs.get(f), fv.weights.get(f), g.get(o), g.get(n)) {
                (Some(&cv), Some(&w), Some(p), Some(q)) => {
                    let mut face = [[0.0; 3]; 3];
                    for ((row, a), b) in face.iter_mut().zip(p).zip(q) {
                        for ((x, &a), &b) in row.iter_mut().zip(a).zip(b) {
                            *x = w * (a - b) + b;
                        }
                    }
                    dot(cv, &face)
                }
                _ => Vec3::default(),
            },
        )
        .collect()
}

/// OpenFOAM's `small`, in `limitedSnGrad`.
const SMALL: f64 = 1e-15;

/// `limitedSnGrad::correction`: the correction of every internal face
/// scaled by `min(k |sn| / ((1 − k) |correction| + small), 1)`, where `sn` is
/// the uncorrected gradient `deltaCoeff (ψ_N − ψ_P)`. `mag` is the norm of a
/// face value.
pub fn limit_correction<T>(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[T],
    correction: &[T],
    k: f64,
    mag: impl Fn(T) -> f64,
) -> Vec<T>
where
    T: std::ops::Mul<f64, Output = T> + std::ops::Sub<Output = T> + Copy,
{
    mesh.owner()
        .iter()
        .zip(mesh.neighbour())
        .zip(correction)
        .enumerate()
        .map(|(f, ((&o, &n), &corr))| {
            let (Some(&po), Some(&pn)) = (cells.get(o), cells.get(n)) else {
                return corr;
            };
            let dc = fv.non_orth_delta_coeffs.get(f).copied().unwrap_or(0.0);
            let sn = mag((pn - po) * dc);
            let limiter = (k * sn / ((1.0 - k) * mag(corr) + SMALL)).min(1.0);
            corr * limiter
        })
        .collect()
}

/// `fvc::snGrad(U)` on every face: across an internal face the cell
/// difference over the distance, plus, for a `Corrected` scheme, the
/// correction (limited by `limit`) from the cells' gradients `g`; on a boundary face what the
/// condition gives (nothing through a zero gradient or an empty patch).
pub fn face_sn_grad_vector(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &VectorBoundary,
    cells: &[Vec3],
    (scheme, limit): (SnGrad, Option<f64>),
    g: &[Tensor],
) -> Vec<Vec3> {
    let coeffs = scheme.coeffs(fv);
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or_default();
    let corr = (scheme == SnGrad::Corrected).then(|| {
        let corr = sn_grad_correction_vector(mesh, fv, g);
        match limit {
            Some(k) => limit_correction(mesh, fv, cells, &corr, k, Vec3::norm),
            None => corr,
        }
    });
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        let c = corr.as_ref().and_then(|c| c.get(f)).copied();
        out.push((n - o) * coeffs.get(f).copied().unwrap_or(0.0) + c.unwrap_or_default());
    }
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let inside = cell(mesh.owner().get(f));
            let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
            out.push(match cond {
                VectorBc::FixedValue(v) => (v.get(i).copied().unwrap_or_default() - inside) * dc,
                VectorBc::Mixed { mask, ref_value } => {
                    let m = mask.get(i).copied().unwrap_or_default();
                    let r = ref_value.get(i).copied().unwrap_or_default() - inside;
                    Vec3::new(m.x * r.x, m.y * r.y, m.z * r.z) * dc
                }
                VectorBc::Wedge { cell_t, .. } => VectorBc::wedge_sn_grad(cell_t, inside, dc),
                VectorBc::ZeroGradient | VectorBc::Empty => Vec3::default(),
            });
        }
    }
    out
}

/// `Γ_f |S_f|` times a correction, on every internal face.
pub fn scale_by_gamma_mag_sf<T: std::ops::Mul<f64, Output = T> + Copy>(
    fv: &FvGeometry,
    gamma: &[f64],
    correction: &[T],
) -> Vec<T> {
    correction
        .iter()
        .enumerate()
        .map(|(f, &c)| {
            c * (gamma.get(f).copied().unwrap_or(0.0) * fv.mag_sf.get(f).copied().unwrap_or(0.0))
        })
        .collect()
}

/// `source −= V ∇·(flux)` for a face flux on the internal faces, which is
/// what `fvm::laplacian` does with `Γ |S| · snGradCorrection`.
pub fn subtract_divergence<T>(source: &mut [T], mesh: &PolyMesh, flux: &[T])
where
    T: std::ops::Add<Output = T> + std::ops::Sub<Output = T> + Copy,
{
    for ((&o, &n), &c) in mesh.owner().iter().zip(mesh.neighbour()).zip(flux) {
        if let Some(s) = source.get_mut(o) {
            *s = *s - c;
        }
        if let Some(s) = source.get_mut(n) {
            *s = *s + c;
        }
    }
}

#[cfg(test)]
mod tests {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box_raw;

    use super::*;
    use crate::limited::gauss_grad;

    /// A 3 × 3 × 1 box sheared into parallelepipeds (`x += y/2`): every face
    /// is off orthogonal, and every face centre is the midpoint of its two
    /// cell centres, so a linear field has an exact Gauss gradient.
    fn sheared() -> (PolyMesh, FvGeometry) {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(3.0, 3.0, 1.0),
        };
        let mut raw = structured_box_raw(bounds, [3, 3, 1]).unwrap();
        for p in &mut raw.points {
            p.x += 0.5 * p.y;
        }
        let mesh = PolyMesh::from_raw(raw).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    /// The face-normal gradient of a linear field is `n · a`; the corrected
    /// one gets there, the uncorrected one (the cell difference over the
    /// distance) does not.
    #[test]
    fn the_correction_makes_a_linear_fields_normal_gradient_exact() {
        let (mesh, fv) = sheared();
        let a = Vec3::new(2.0, 3.0, 0.0);
        let value = |c: Vec3| a.dot(c) + 1.0;
        let cells: Vec<f64> = fv.geo.cell_centres.iter().map(|&c| value(c)).collect();
        let boundary: Vec<Option<f64>> = (mesh.n_internal_faces()..mesh.n_faces())
            .map(|f| fv.geo.face_centres.get(f).map(|&c| value(c)))
            .collect();
        let g = gauss_grad(&mesh, &fv, &cells, &boundary);
        let corr = sn_grad_correction(&mesh, &fv, &g);
        assert_eq!(corr.len(), mesh.n_internal_faces());
        let mut off = 0.0f64;
        for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
            let (Some(&s), Some(&mag), Some(&dc), Some(&c)) = (
                fv.geo.face_areas.get(f),
                fv.mag_sf.get(f),
                fv.non_orth_delta_coeffs.get(f),
                corr.get(f),
            ) else {
                continue;
            };
            let diff = dc * (cells[n] - cells[o]);
            let exact = a.dot(s) / mag;
            assert!((diff + c - exact).abs() < 1e-12, "face {f}");
            off = off.max((diff - exact).abs());
        }
        assert!(off > 1e-3, "the mesh is not non-orthogonal: {off}");
    }

    /// `limited 0` takes the whole correction away, `limited 1` keeps it
    /// wherever the uncorrected gradient is not zero, and in between the
    /// correction stays below `k / (1 − k)` times that gradient.
    #[test]
    fn a_limited_correction_stays_within_its_share_of_the_gradient() {
        let (mesh, fv) = sheared();
        let a = Vec3::new(2.0, 3.0, 0.0);
        let cells: Vec<f64> = fv.geo.cell_centres.iter().map(|&c| a.dot(c)).collect();
        let boundary: Vec<Option<f64>> = (mesh.n_internal_faces()..mesh.n_faces())
            .map(|f| fv.geo.face_centres.get(f).map(|&c| a.dot(c)))
            .collect();
        let g = gauss_grad(&mesh, &fv, &cells, &boundary);
        let corr = sn_grad_correction(&mesh, &fv, &g);
        let limit = |k: f64| limit_correction(&mesh, &fv, &cells, &corr, k, f64::abs);
        assert!(limit(0.0).iter().all(|&c| c == 0.0));
        for (f, (&full, &whole)) in corr.iter().zip(&limit(1.0)).enumerate() {
            let sn =
                fv.non_orth_delta_coeffs[f] * (cells[mesh.neighbour()[f]] - cells[mesh.owner()[f]]);
            if sn != 0.0 {
                assert_eq!(full, whole, "face {f}");
            }
        }
        let k = 0.25;
        let mut cut = 0;
        for (f, (&full, &part)) in corr.iter().zip(&limit(k)).enumerate() {
            let sn =
                fv.non_orth_delta_coeffs[f] * (cells[mesh.neighbour()[f]] - cells[mesh.owner()[f]]);
            assert!(
                part.abs() <= k / (1.0 - k) * sn.abs() * (1.0 + 1e-12),
                "face {f}"
            );
            assert!(part.abs() <= full.abs() * (1.0 + 1e-12), "face {f}");
            cut += usize::from(part.abs() < full.abs());
        }
        assert!(cut > 0, "nothing was limited");
    }

    /// `source −= V ∇·flux` puts a face's flux on its owner and takes it from
    /// its neighbour, and `Γ |S|` scales it.
    #[test]
    fn a_face_flux_leaves_the_neighbour_and_enters_the_owner() {
        let (mesh, fv) = sheared();
        let gamma = vec![2.0; mesh.n_faces()];
        let mut ones = vec![0.0; mesh.n_internal_faces()];
        if let Some(x) = ones.first_mut() {
            *x = 1.0;
        }
        let flux = scale_by_gamma_mag_sf(&fv, &gamma, &ones);
        let mut source = vec![0.0; mesh.n_cells()];
        subtract_divergence(&mut source, &mesh, &flux);
        let (Some(&o), Some(&n), Some(&mag)) = (
            mesh.owner().first(),
            mesh.neighbour().first(),
            fv.mag_sf.first(),
        ) else {
            return;
        };
        let at = |c: usize| source.get(c).copied().unwrap_or(f64::NAN);
        assert!((at(o) + 2.0 * mag).abs() < 1e-12, "owner {o}: {}", at(o));
        assert!(
            (at(n) - 2.0 * mag).abs() < 1e-12,
            "neighbour {n}: {}",
            at(n)
        );
        assert!(source.iter().sum::<f64>().abs() < 1e-12);
    }
}
