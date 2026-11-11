//! The explicit part of the viscous stress of a Newtonian fluid,
//! `∇·(ν dev2(∇Uᵀ))`, which `divDevReff` puts beside the implicit
//! `∇·(ν ∇U)`.
//!
//! Each step follows OpenFOAM's (GPL-3.0-or-later) floating-point order:
//! the Gauss gradient with its boundary values corrected to the patch's
//! normal gradient, then `dev2` of the transpose, then the divergence by
//! linear interpolation.

use vortia_core::Vec3;
use vortia_mesh::{PatchKind, PolyMesh};

use crate::fvgeom::FvGeometry;
use crate::vector::{VectorBc, VectorBoundary};

/// A second-rank tensor, row by row: `t[i][j]`.
pub type Tensor = [[f64; 3]; 3];

/// `a ⊗ b`.
fn outer(a: Vec3, b: Vec3) -> Tensor {
    let (a, b) = ([a.x, a.y, a.z], [b.x, b.y, b.z]);
    let mut t = [[0.0; 3]; 3];
    for (row, &ai) in t.iter_mut().zip(&a) {
        for (cell, &bj) in row.iter_mut().zip(&b) {
            *cell = ai * bj;
        }
    }
    t
}

fn zip_with(a: &Tensor, b: &Tensor, f: impl Fn(f64, f64) -> f64) -> Tensor {
    let mut t = [[0.0; 3]; 3];
    for ((row, ra), rb) in t.iter_mut().zip(a).zip(b) {
        for ((cell, &x), &y) in row.iter_mut().zip(ra).zip(rb) {
            *cell = f(x, y);
        }
    }
    t
}

/// `v & t`: `Σᵢ vᵢ tᵢⱼ`, summed in OpenFOAM's order.
fn dot(v: Vec3, t: &Tensor) -> Vec3 {
    let [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = *t;
    Vec3::new(
        v.x * xx + v.y * yx + v.z * zx,
        v.x * xy + v.y * yy + v.z * zy,
        v.x * xz + v.y * yz + v.z * zz,
    )
}

/// `dev2(tᵀ)`: the transpose less two thirds of its trace on the diagonal.
fn dev2_transpose(t: &Tensor) -> Tensor {
    let [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = *t;
    let third = (2.0 / 3.0) * (xx + yy + zz);
    [
        [xx - third, yx, zx],
        [xy, yy - third, zy],
        [xz, yz, zz - third],
    ]
}

/// The Gauss gradient of `u` in each cell, and on each boundary face (`None`
/// on empty faces) the cell's gradient with its normal part replaced by
/// the patch's normal gradient, as `gaussGrad::correctBoundaryConditions`
/// does.
pub fn grad(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &VectorBoundary,
    u: &[Vec3],
) -> (Vec<Tensor>, Vec<Option<Tensor>>) {
    let mut g = vec![[[0.0; 3]; 3]; mesh.n_cells()];
    let sf = &fv.geo.face_areas;
    let own = mesh.owner();
    let cell = |c: Option<&usize>| c.and_then(|&c| u.get(c)).copied().unwrap_or_default();
    for (f, (&o, &n)) in own.iter().zip(mesh.neighbour()).enumerate() {
        let (Some(&w), Some(&s)) = (fv.weights.get(f), sf.get(f)) else {
            continue;
        };
        let (p, q) = (cell(Some(&o)), cell(Some(&n)));
        // `w·P + (1 − w)·N`, as for the scalar gradient.
        let face = Vec3::new(
            w * p.x + (1.0 - w) * q.x,
            w * p.y + (1.0 - w) * q.y,
            w * p.z + (1.0 - w) * q.z,
        );
        let t = outer(s, face);
        if let Some(go) = g.get_mut(o) {
            *go = zip_with(go, &t, |a, b| a + b);
        }
        if let Some(gn) = g.get_mut(n) {
            *gn = zip_with(gn, &t, |a, b| a - b);
        }
    }
    let boundary = bc.face_values(mesh, u);
    let n_internal = mesh.n_internal_faces();
    for (k, v) in boundary.iter().enumerate() {
        let f = n_internal + k;
        let (Some(v), Some(&s), Some(&o)) = (v, sf.get(f), own.get(f)) else {
            continue;
        };
        if let Some(go) = g.get_mut(o) {
            *go = zip_with(go, &outer(s, *v), |a, b| a + b);
        }
    }
    for (gc, &vol) in g.iter_mut().zip(fv.volumes()) {
        *gc = zip_with(gc, &[[vol; 3]; 3], |a, b| a / b);
    }

    let mut out = Vec::with_capacity(boundary.len());
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            if patch.kind == PatchKind::Empty {
                out.push(None);
                continue;
            }
            let c = own.get(f).copied();
            let gc = c.and_then(|c| g.get(c)).copied().unwrap_or_default();
            let s = sf.get(f).copied().unwrap_or_default();
            let mag = fv.mag_sf.get(f).copied().unwrap_or(1.0);
            let n = Vec3::new(s.x / mag, s.y / mag, s.z / mag);
            let inside = cell(c.as_ref());
            let sn = match cond {
                VectorBc::FixedValue(v) => {
                    let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                    let b = v.get(i).copied().unwrap_or_default();
                    Vec3::new(
                        dc * (b.x - inside.x),
                        dc * (b.y - inside.y),
                        dc * (b.z - inside.z),
                    )
                }
                VectorBc::ZeroGradient | VectorBc::Empty => Vec3::default(),
            };
            let normal = dot(n, &gc);
            let fix = outer(n, sn - normal);
            out.push(Some(zip_with(&gc, &fix, |a, b| a + b)));
        }
    }
    (g, out)
}

/// `dev(twoSymm(g)) && g`: what `ν_t` multiplies in the production of
/// turbulent kinetic energy, `G = ν_t (dev(twoSymm(∇U)) && ∇U)`.
pub fn production(g: &Tensor) -> f64 {
    let [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = *g;
    let (sxx, sxy, sxz) = (2.0 * xx, xy + yx, xz + zx);
    let (syy, syz, szz) = (2.0 * yy, yz + zy, 2.0 * zz);
    let third = (1.0 / 3.0) * (sxx + syy + szz);
    let (sxx, syy, szz) = (sxx - third, syy - third, szz - third);
    sxx * xx + sxy * xy + sxz * xz + sxy * yx + syy * yy + syz * yz + sxz * zx + syz * zy + szz * zz
}

/// `fvc::div(ν dev2(T(fvc::grad(U))))` with `Gauss linear`, per cell, for
/// `nu` in every cell and on every boundary face (`None` on empty faces).
pub fn div_dev2_grad_t(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &VectorBoundary,
    u: &[Vec3],
    nu: &[f64],
    nu_boundary: &[Option<f64>],
) -> Vec<Vec3> {
    let (cells, boundary) = grad(mesh, fv, bc, u);
    let stress = |t: &Tensor, nu: f64| zip_with(&dev2_transpose(t), &[[nu; 3]; 3], |a, n| n * a);
    let cells: Vec<Tensor> = cells.iter().zip(nu).map(|(t, &n)| stress(t, n)).collect();
    let sf = &fv.geo.face_areas;
    let own = mesh.owner();
    let mut div = vec![Vec3::default(); mesh.n_cells()];
    for (f, (&o, &n)) in own.iter().zip(mesh.neighbour()).enumerate() {
        let (Some(&w), Some(&s), Some(p), Some(q)) =
            (fv.weights.get(f), sf.get(f), cells.get(o), cells.get(n))
        else {
            continue;
        };
        let face = zip_with(p, q, |p, q| w * (p - q) + q);
        let flux = dot(s, &face);
        if let Some(d) = div.get_mut(o) {
            *d = *d + flux;
        }
        if let Some(d) = div.get_mut(n) {
            *d = *d - flux;
        }
    }
    let n_internal = mesh.n_internal_faces();
    for (k, t) in boundary.iter().enumerate() {
        let f = n_internal + k;
        let (Some(t), Some(&s), Some(&o), Some(Some(n))) =
            (t, sf.get(f), own.get(f), nu_boundary.get(k))
        else {
            continue;
        };
        if let Some(d) = div.get_mut(o) {
            *d = *d + dot(s, &stress(t, *n));
        }
    }
    for (d, &vol) in div.iter_mut().zip(fv.volumes()) {
        *d = Vec3::new(d.x / vol, d.y / vol, d.z / vol);
    }
    div
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev2_of_the_transpose() {
        let t = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let d = dev2_transpose(&t);
        assert_eq!(d[0][1], 4.0);
        assert_eq!(d[1][0], 2.0);
        assert_eq!(d[0][0], 1.0 - 10.0);
        assert_eq!(dot(Vec3::new(1.0, 0.0, 0.0), &t), Vec3::new(1.0, 2.0, 3.0));
        // Simple shear du/dy = 1: 2 S:S = 1.
        let shear = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        assert_eq!(production(&shear), 1.0);
    }

    /// On a sheared cell the boundary gradient's normal part is the patch's
    /// normal gradient over the normal distance (½ here), not over the
    /// centre-to-face distance (√½).
    #[test]
    fn a_skewed_wall_takes_its_normal_gradient_over_the_normal_distance() {
        use vortia_core::Aabb;
        use vortia_mesh::blocks::structured_box_raw;

        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let mut raw = structured_box_raw(bounds, [1, 1, 1]).unwrap();
        for p in &mut raw.points {
            p.x += p.y;
        }
        let mesh = PolyMesh::from_raw(raw).unwrap();
        let fv = FvGeometry::new(&mesh);
        let conditions = mesh
            .patches()
            .iter()
            .map(|p| match p.name.as_str() {
                "yMin" => VectorBc::FixedValue(vec![Vec3::new(1.0, 0.0, 0.0); p.size]),
                _ => VectorBc::ZeroGradient,
            })
            .collect();
        let bc = VectorBoundary { conditions };
        let (_, faces) = grad(&mesh, &fv, &bc, &[Vec3::default()]);
        let y_min = mesh.patch("yMin").unwrap();
        let k = y_min.start - mesh.n_internal_faces();
        let g = faces[k].unwrap();
        let normal = dot(Vec3::new(0.0, -1.0, 0.0), &g);
        assert!((normal.x - 2.0).abs() < 1e-12, "{normal:?}");
    }
}
