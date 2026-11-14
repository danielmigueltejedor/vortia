//! Face fields of cell fields: the face-normal gradient `fvc::snGrad`, and
//! the cell vector `fvc::reconstruct` rebuilds from a flux through the faces.
//!
//! Derived from OpenFOAM's `snGradScheme` and `fvcReconstruct` (OpenFOAM,
//! Copyright (C) 2011-2017 OpenFOAM Foundation and OpenCFD Ltd.,
//! GPL-3.0-or-later). Both give one value per mesh face: the internal faces
//! first, then the boundary faces in patch order.

use vortia_core::Vec3;
use vortia_mesh::{PatchKind, PolyMesh};

use crate::bc::{Boundary, ScalarBc};
use crate::correction;
use crate::fvgeom::FvGeometry;
use crate::ops::{self, SnGrad};

/// `fvc::snGrad(ψ)` on every face: across the internal faces, and from the
/// conditions on the boundary (none through a zero gradient).
pub fn sn_grad(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    cells: &[f64],
    scheme: SnGrad,
) -> Vec<f64> {
    let coeffs = scheme.coeffs(fv);
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or(0.0);
    // `correctedSnGrad`: the interpolated cell gradient along the
    // non-orthogonal part of the face vector, added to the difference.
    let corr = (scheme == SnGrad::Corrected).then(|| {
        let g = ops::grad(mesh, fv, bc, cells);
        correction::sn_grad_correction(mesh, fv, &g)
    });
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        let c = corr.as_ref().and_then(|c| c.get(f)).copied();
        out.push(coeffs.get(f).copied().unwrap_or(0.0) * (n - o) + c.unwrap_or(0.0));
    }
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let inside = cell(mesh.owner().get(f));
            let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
            out.push(match cond {
                ScalarBc::FixedValue(v) | ScalarBc::Calculated(v) => {
                    dc * (v.get(i).copied().unwrap_or(0.0) - inside)
                }
                ScalarBc::FixedGradient(g) => g.get(i).copied().unwrap_or(0.0),
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let f_m = fraction.get(i).copied().unwrap_or(0.0);
                    f_m * dc * (ref_value.get(i).copied().unwrap_or(0.0) - inside)
                }
                ScalarBc::ZeroGradient | ScalarBc::Reflect | ScalarBc::Empty => 0.0,
            });
        }
    }
    out
}

type Tensor = [[f64; 3]; 3];

/// The inverse of a symmetric tensor, with OpenFOAM's treatment of a
/// direction the mesh does not have (a 2D case): a zero diagonal entry is
/// taken as one, so the inverse has one there and the vector it is applied
/// to, which has nothing in that direction, stays without. `None` for a
/// tensor that is not invertible.
fn inverse(mut m: Tensor) -> Option<Tensor> {
    let scale = m[0][0].max(m[1][1]).max(m[2][2]);
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    for (d, row) in m.iter_mut().enumerate() {
        if let Some(x) = row.get_mut(d)
            && *x < 1e-12 * scale
        {
            *x += 1.0;
        }
    }
    let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
    let c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2];
    let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
    let det = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
    if !det.is_finite() || det.abs() < 1e-300 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        [
            c00 * inv,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
        ],
        [
            c01 * inv,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
        ],
        [
            c02 * inv,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
        ],
    ])
}

/// `fvc::reconstruct(ssf)`: the cell vector whose flux through the faces is
/// `ssf` in the least-squares sense,
/// `(Σ Sf⊗Sf/|Sf|)⁻¹ · Σ Sf/|Sf| · ssf`, the sums over each cell's faces
/// (those of empty patches left out). `ssf` is given on every face; a cell
/// whose faces do not span the space gets zero.
pub fn reconstruct(mesh: &PolyMesh, fv: &FvGeometry, ssf: &[f64]) -> Vec<Vec3> {
    let n = mesh.n_cells();
    let mut tensors: Vec<Tensor> = vec![[[0.0; 3]; 3]; n];
    let mut sums = vec![Vec3::default(); n];
    let mut add = |cell: usize, f: usize| {
        let (Some(&sf), Some(&mag), Some(&flux)) =
            (fv.geo.face_areas.get(f), fv.mag_sf.get(f), ssf.get(f))
        else {
            return;
        };
        if mag <= 0.0 {
            return;
        }
        let a = [sf.x, sf.y, sf.z];
        if let Some(t) = tensors.get_mut(cell) {
            for (row, &ai) in t.iter_mut().zip(&a) {
                for (x, &aj) in row.iter_mut().zip(&a) {
                    *x += ai * aj / mag;
                }
            }
        }
        if let Some(s) = sums.get_mut(cell) {
            *s = *s + (sf / mag) * flux;
        }
    };
    for (f, (&o, &nb)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        add(o, f);
        add(nb, f);
    }
    for patch in mesh.patches() {
        if patch.kind == PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            if let Some(&o) = mesh.owner().get(f) {
                add(o, f);
            }
        }
    }
    tensors
        .into_iter()
        .zip(sums)
        .map(|(t, s)| match inverse(t) {
            Some(i) => Vec3::new(
                i[0][0] * s.x + i[0][1] * s.y + i[0][2] * s.z,
                i[1][0] * s.x + i[1][1] * s.y + i[1][2] * s.z,
                i[2][0] * s.x + i[2][1] * s.y + i[2][2] * s.z,
            ),
            None => Vec3::default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_direction_is_inverted_as_one() {
        let i = inverse([[2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 0.0]]).unwrap();
        assert_eq!(i, [[0.5, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 1.0]]);
    }

    #[test]
    fn a_full_tensor_is_inverted() {
        let m = [[2.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 4.0]];
        let i = inverse(m).unwrap();
        for (r, row) in m.iter().enumerate() {
            for c in 0..3 {
                let x: f64 = row.iter().zip(&i).map(|(&a, col)| a * col[c]).sum();
                assert!(
                    (x - f64::from(u8::from(r == c))).abs() < 1e-12,
                    "{r}{c}: {x}"
                );
            }
        }
    }

    #[test]
    fn nothing_to_invert_is_refused() {
        assert!(inverse([[0.0; 3]; 3]).is_none());
        assert!(inverse([[f64::NAN; 3]; 3]).is_none());
    }
}
