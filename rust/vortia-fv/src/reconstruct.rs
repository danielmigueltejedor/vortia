//! Directed face values of a limited interpolation scheme: what
//! `rhoCentralFoam`'s `interpolate(field, pos)` and `interpolate(field,
//! neg)` give, the value reconstructed from the owner side (`+1`) and from
//! the neighbour side (`-1`).
//!
//! Derived from OpenFOAM's `LimitedScheme`, `limitedSurfaceInterpolationScheme`,
//! `NVDTVD`, `NVDVTVDV`, `vanLeer` and `Minmod` (OpenFOAM, Copyright (C)
//! 2011-2017 OpenFOAM Foundation and OpenCFD Ltd., GPL-3.0-or-later), in
//! their floating-point order: a limiter per face from the gradient of the
//! field in the cell upwind of the direction, the weight `λ L + (1 − L)
//! pos0(dir)`, and the face value `w (P − N) + N`. Boundary faces take the
//! field's own value.

use vortia_core::Vec3;
use vortia_mesh::PolyMesh;

use crate::error::{FvError, Result};
use crate::fvgeom::FvGeometry;
use crate::limited::{self, GradScheme};
use crate::stress::{Tensor, dot, limit_cells};

/// The limiter function of a scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limiter {
    VanLeer,
    Minmod,
}

impl Limiter {
    fn value(self, r: f64) -> f64 {
        match self {
            Self::VanLeer => (r + r.abs()) / (1.0 + r.abs()),
            // `max(min(r, 1), 0)`: a NaN ratio gives 1, which `clamp` would not.
            #[allow(clippy::manual_clamp)]
            Self::Minmod => r.min(1.0).max(0.0),
        }
    }
}

/// An `interpolationSchemes` entry for a reconstructed field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconstruction {
    /// The value of the cell the flow leaves.
    Upwind,
    Linear,
    /// A limited scheme. `vector` is the `V` form (`vanLeerV`), whose ratio
    /// of gradients follows the change of the vector itself; without it a
    /// vector field is limited by its squared magnitude, as `vanLeer` and
    /// `Minmod` do.
    Limited {
        limiter: Limiter,
        vector: bool,
    },
}

impl Reconstruction {
    /// From the words of the entry, for a scalar field or a vector one.
    pub fn from_scheme(words: &[&str], vector_field: bool) -> Result<Self> {
        let limited = |limiter, v: bool| {
            if v && !vector_field {
                Err(FvError::Setup(format!(
                    "interpolation scheme '{}' is for vector fields",
                    words.join(" ")
                )))
            } else {
                Ok(Self::Limited { limiter, vector: v })
            }
        };
        match words {
            ["upwind"] => Ok(Self::Upwind),
            ["linear"] => Ok(Self::Linear),
            ["vanLeer"] => limited(Limiter::VanLeer, false),
            ["vanLeerV"] => limited(Limiter::VanLeer, true),
            ["Minmod"] => limited(Limiter::Minmod, false),
            other => Err(FvError::Setup(format!(
                "interpolation scheme '{}' is not supported yet; use upwind, linear, \
                 vanLeer, vanLeerV or Minmod",
                other.join(" ")
            ))),
        }
    }

    /// `limiter·λ + (1 − limiter)·pos0(dir)` on every internal face for the
    /// scalar `cells` (or the squared magnitude of a vector), given the
    /// boundary values of the field for its gradient, which `grad` names the
    /// scheme of.
    pub fn weights(
        self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        direction: f64,
        cells: &[f64],
        boundary: &[Option<f64>],
        grad: GradScheme,
    ) -> Vec<f64> {
        match self {
            Self::Upwind | Self::Linear => self.fixed(mesh, fv, direction),
            Self::Limited { limiter, .. } => {
                let g = limited::grad(grad, mesh, fv, cells, boundary);
                self.combine(mesh, fv, direction, limiter, |o, n, d| {
                    let (Some(&po), Some(&pn)) = (cells.get(o), cells.get(n)) else {
                        return 1.0;
                    };
                    let gradf = pn - po;
                    let gradc = if direction > 0.0 { g.get(o) } else { g.get(n) };
                    ratio(d.dot(gradc.copied().unwrap_or_default()), gradf)
                })
            }
        }
    }

    /// The same for a vector field in the `V` form, from the gradients of
    /// its cells.
    pub fn weights_vector(
        self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        direction: f64,
        cells: &[Vec3],
        boundary: &[Option<Vec3>],
        grad: GradScheme,
    ) -> Vec<f64> {
        match self {
            Self::Limited {
                limiter,
                vector: true,
            } => {
                let mut g = gauss_grad_vector(mesh, fv, cells, boundary);
                if let GradScheme::CellLimited(k) = grad {
                    limit_cells(mesh, fv, cells, boundary, &mut g, k);
                }
                self.combine(mesh, fv, direction, limiter, |o, n, d| {
                    let (Some(&po), Some(&pn)) = (cells.get(o), cells.get(n)) else {
                        return 1.0;
                    };
                    let gradf_v = pn - po;
                    let gradf = gradf_v.dot(gradf_v);
                    let up = if direction > 0.0 { g.get(o) } else { g.get(n) };
                    let t = up.copied().unwrap_or([[0.0; 3]; 3]);
                    ratio(gradf_v.dot(dot(d, &t)), gradf)
                })
            }
            Self::Limited { vector: false, .. } => {
                let mag_sqr: Vec<f64> = cells.iter().map(|v| v.dot(*v)).collect();
                let boundary: Vec<Option<f64>> =
                    boundary.iter().map(|v| v.map(|v| v.dot(v))).collect();
                self.weights(mesh, fv, direction, &mag_sqr, &boundary, grad)
            }
            Self::Upwind | Self::Linear => self.fixed(mesh, fv, direction),
        }
    }

    fn fixed(self, mesh: &PolyMesh, fv: &FvGeometry, direction: f64) -> Vec<f64> {
        (0..mesh.n_internal_faces())
            .map(|f| match self {
                Self::Linear => fv.weights.get(f).copied().unwrap_or(0.5),
                _ => pos0(direction),
            })
            .collect()
    }

    fn combine(
        self,
        mesh: &PolyMesh,
        fv: &FvGeometry,
        direction: f64,
        limiter: Limiter,
        r: impl Fn(usize, usize, Vec3) -> f64,
    ) -> Vec<f64> {
        let centres = &fv.geo.cell_centres;
        mesh.owner()
            .iter()
            .zip(mesh.neighbour())
            .enumerate()
            .map(|(f, (&o, &n))| {
                let d = match (centres.get(o), centres.get(n)) {
                    (Some(&co), Some(&cn)) => cn - co,
                    _ => Vec3::default(),
                };
                let l = limiter.value(r(o, n, d));
                let w = fv.weights.get(f).copied().unwrap_or(0.5);
                l * w + (1.0 - l) * pos0(direction)
            })
            .collect()
    }
}

/// `pos0`: 1 where `x` is not negative.
fn pos0(x: f64) -> f64 {
    if x >= 0.0 { 1.0 } else { 0.0 }
}

/// `sign`: 1 where `x` is not negative, else −1.
fn sign(x: f64) -> f64 {
    if x >= 0.0 { 1.0 } else { -1.0 }
}

/// `NVDTVD::r`: twice the ratio of the upwind gradient along the cell
/// distance to the difference across the face, less one, saturated where the
/// difference is a thousandth of the gradient or less.
fn ratio(gradcf: f64, gradf: f64) -> f64 {
    if gradcf.abs() >= 1000.0 * gradf.abs() {
        2.0 * 1000.0 * sign(gradcf) * sign(gradf) - 1.0
    } else {
        2.0 * (gradcf / gradf) - 1.0
    }
}

/// `w (P − N) + N` on every internal face with the weights given, then the
/// boundary values (`0` where `None`).
pub fn interpolate<T>(
    mesh: &PolyMesh,
    weights: &[f64],
    cells: &[T],
    boundary: &[Option<T>],
) -> Vec<T>
where
    T: Copy + Default + std::ops::Sub<Output = T> + std::ops::Add<Output = T> + Scale,
{
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or_default();
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let w = weights.get(f).copied().unwrap_or(0.5);
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        out.push((o - n).scale(w) + n);
    }
    out.extend(boundary.iter().map(|v| v.unwrap_or_default()));
    out
}

/// A value a weight multiplies.
pub trait Scale {
    fn scale(self, w: f64) -> Self;
}

impl Scale for f64 {
    fn scale(self, w: f64) -> Self {
        w * self
    }
}

impl Scale for Vec3 {
    fn scale(self, w: f64) -> Self {
        Self::new(w * self.x, w * self.y, w * self.z)
    }
}

/// The Gauss gradient of a vector field, `∂ᵢ uⱼ` in `t[i][j]`, with the
/// face values `w P + (1 − w) N`, as for a scalar.
fn gauss_grad_vector(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[Vec3],
    boundary: &[Option<Vec3>],
) -> Vec<Tensor> {
    let mut g = vec![[[0.0; 3]; 3]; mesh.n_cells()];
    let sf = &fv.geo.face_areas;
    let own = mesh.owner();
    let add = |t: &mut Tensor, s: Vec3, v: Vec3, plus: bool| {
        for (row, si) in t.iter_mut().zip([s.x, s.y, s.z]) {
            for (cell, vj) in row.iter_mut().zip([v.x, v.y, v.z]) {
                let x = si * vj;
                if plus {
                    *cell += x;
                } else {
                    *cell -= x;
                }
            }
        }
    };
    for (f, (&o, &n)) in own.iter().zip(mesh.neighbour()).enumerate() {
        let (Some(&w), Some(&s), Some(&p), Some(&q)) =
            (fv.weights.get(f), sf.get(f), cells.get(o), cells.get(n))
        else {
            continue;
        };
        let face = Vec3::new(
            w * p.x + (1.0 - w) * q.x,
            w * p.y + (1.0 - w) * q.y,
            w * p.z + (1.0 - w) * q.z,
        );
        if let Some(go) = g.get_mut(o) {
            add(go, s, face, true);
        }
        if let Some(gn) = g.get_mut(n) {
            add(gn, s, face, false);
        }
    }
    for (k, v) in boundary.iter().enumerate() {
        let f = mesh.n_internal_faces() + k;
        let (Some(v), Some(&s), Some(&o)) = (v, sf.get(f), own.get(f)) else {
            continue;
        };
        if let Some(go) = g.get_mut(o) {
            add(go, s, *v, true);
        }
    }
    for (t, &vol) in g.iter_mut().zip(fv.volumes()) {
        if vol > 0.0 {
            for cell in t.iter_mut().flatten() {
                *cell /= vol;
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use vortia_core::Aabb;
    use vortia_mesh::blocks::structured_box;

    use super::*;

    fn line(n: usize) -> (PolyMesh, FvGeometry) {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(n as f64, 1.0, 1.0),
        };
        let mesh = structured_box(bounds, [n, 1, 1]).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    #[test]
    fn a_smooth_ramp_is_reconstructed_linearly_and_a_step_upwind() {
        let (mesh, fv) = line(8);
        let ramp: Vec<f64> = (0..8).map(f64::from).collect();
        let bc: Vec<Option<f64>> = vec![None; mesh.n_faces() - mesh.n_internal_faces()];
        let scheme = Reconstruction::Limited {
            limiter: Limiter::VanLeer,
            vector: false,
        };
        let w = scheme.weights(&mesh, &fv, 1.0, &ramp, &bc, GradScheme::GaussLinear);
        // On a ramp r = 1, so the limiter is 1 and the weights are linear.
        for (f, x) in w.iter().enumerate().skip(1).take(w.len().saturating_sub(2)) {
            assert!((x - fv.weights[f]).abs() < 1e-12, "face {f}: {x}");
        }
        // A step is upwind: the limiter is 0 beside the jump.
        let step = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let w = scheme.weights(&mesh, &fv, 1.0, &step, &bc, GradScheme::GaussLinear);
        let right = scheme.weights(&mesh, &fv, -1.0, &step, &bc, GradScheme::GaussLinear);
        assert!(w[3] <= 1.0 && w[3] >= 0.0);
        assert!(
            (w[3] - 1.0).abs() < 1e-12,
            "owner side at the jump: {}",
            w[3]
        );
        assert!(
            right[3].abs() < 1e-12,
            "neighbour side at the jump: {}",
            right[3]
        );
    }

    #[test]
    fn upwind_takes_the_side_the_direction_names() {
        let (mesh, fv) = line(4);
        let owner =
            Reconstruction::Upwind.weights(&mesh, &fv, 1.0, &[], &[], GradScheme::GaussLinear);
        let neighbour =
            Reconstruction::Upwind.weights(&mesh, &fv, -1.0, &[], &[], GradScheme::GaussLinear);
        assert!(owner.iter().all(|&w| w == 1.0));
        assert!(neighbour.iter().all(|&w| w == 0.0));
        let values = [1.0, 2.0, 3.0, 4.0];
        let out = interpolate(&mesh, &owner, &values, &[]);
        assert_eq!(&out[..3], &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn the_vector_form_and_the_scalar_form_agree_on_one_direction() {
        let (mesh, fv) = line(8);
        let xs = [0.0, 0.1, 0.3, 0.9, 1.0, 1.4, 1.5, 1.6];
        let vs: Vec<Vec3> = xs.iter().map(|&x| Vec3::new(x, 0.0, 0.0)).collect();
        let nb = mesh.n_faces() - mesh.n_internal_faces();
        let v = Reconstruction::Limited {
            limiter: Limiter::VanLeer,
            vector: true,
        }
        .weights_vector(
            &mesh,
            &fv,
            1.0,
            &vs,
            &vec![None; nb],
            GradScheme::GaussLinear,
        );
        let s = Reconstruction::Limited {
            limiter: Limiter::VanLeer,
            vector: false,
        }
        .weights(
            &mesh,
            &fv,
            1.0,
            &xs,
            &vec![None; nb],
            GradScheme::GaussLinear,
        );
        for (a, b) in v.iter().zip(&s) {
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
    }

    #[test]
    fn hostile_values_do_not_panic() {
        let (mesh, fv) = line(4);
        let nb = mesh.n_faces() - mesh.n_internal_faces();
        let scheme = Reconstruction::Limited {
            limiter: Limiter::Minmod,
            vector: false,
        };
        for cells in [
            vec![f64::NAN; 4],
            vec![f64::INFINITY, 0.0, f64::NEG_INFINITY, 1.0],
            vec![1.0; 2],
            vec![],
        ] {
            let _ = scheme.weights(
                &mesh,
                &fv,
                1.0,
                &cells,
                &vec![None; nb],
                GradScheme::GaussLinear,
            );
            let _ = scheme.weights(&mesh, &fv, -1.0, &cells, &[], GradScheme::GaussLinear);
        }
        assert!(Reconstruction::from_scheme(&["vanLeerV"], false).is_err());
        assert!(Reconstruction::from_scheme(&["QUICK"], true).is_err());
        assert!(Reconstruction::from_scheme(&[], true).is_err());
    }
}
