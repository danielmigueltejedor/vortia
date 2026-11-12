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
use crate::stress::{Tensor, dot};

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
