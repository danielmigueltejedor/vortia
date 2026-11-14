//! An equation in a scalar field kept as OpenFOAM's `fvMatrix` keeps it: the
//! matrix and source of the cells, and the boundary's coefficients apart, so
//! under-relaxation and fixing values in cells see them as OpenFOAM does.
//!
//! Derived from OpenFOAM's `fvMatrix`, `fvm::Sp`, `fvm::SuSp`, the Gauss
//! convection and Laplacian schemes (OpenFOAM, GPL-3.0-or-later). Each sum is
//! taken in OpenFOAM's order, so results agree to rounding.

use vortia_mesh::{PatchKind, PolyMesh};

use crate::bc::{Boundary, ScalarBc};
use crate::error::{FvError, Result};
use crate::fvgeom::FvGeometry;
use crate::ldu::LduMatrix;
use crate::ops::{Interpolation, SnGrad, surface_integrate};
use crate::solve::Performance;

/// `A ψ = source`, with one internal and one boundary coefficient per
/// boundary face (`internalCoeffs`, `boundaryCoeffs`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarMatrix {
    /// The cells' matrix; its `source` is the equation's source.
    pub ldu: LduMatrix,
    /// Added to the diagonal when solving.
    pub internal_coeffs: Vec<f64>,
    /// Added to the source when solving.
    pub boundary_coeffs: Vec<f64>,
}

impl ScalarMatrix {
    /// An all-zero equation on `mesh`.
    pub fn new(mesh: &PolyMesh) -> Self {
        let nb = mesh.n_faces().saturating_sub(mesh.n_internal_faces());
        Self {
            ldu: LduMatrix::new(mesh),
            internal_coeffs: vec![0.0; nb],
            boundary_coeffs: vec![0.0; nb],
        }
    }

    fn zip(a: &mut [f64], b: &[f64], f: impl Fn(f64, f64) -> f64) {
        for (x, &y) in a.iter_mut().zip(b) {
            *x = f(*x, y);
        }
    }

    fn combine(&mut self, other: &Self, f: impl Fn(f64, f64) -> f64 + Copy) {
        Self::zip(&mut self.ldu.diag, &other.ldu.diag, f);
        Self::zip(&mut self.ldu.upper, &other.ldu.upper, f);
        Self::zip(&mut self.ldu.lower, &other.ldu.lower, f);
        Self::zip(&mut self.ldu.source, &other.ldu.source, f);
        Self::zip(&mut self.internal_coeffs, &other.internal_coeffs, f);
        Self::zip(&mut self.boundary_coeffs, &other.boundary_coeffs, f);
    }

    /// `self + other`, term by term.
    pub fn add(&mut self, other: &Self) {
        self.combine(other, |a, b| a + b);
    }

    /// `self − other`, term by term.
    pub fn sub(&mut self, other: &Self) {
        self.combine(other, |a, b| a - b);
    }

    /// `−self`.
    pub fn negate(&mut self) {
        for v in self
            .ldu
            .diag
            .iter_mut()
            .chain(&mut self.ldu.upper)
            .chain(&mut self.ldu.lower)
            .chain(&mut self.ldu.source)
            .chain(&mut self.internal_coeffs)
            .chain(&mut self.boundary_coeffs)
        {
            *v = -*v;
        }
    }

    /// Takes `V·su` from the source: adds the explicit source `su` to the
    /// left-hand side, as `fvMatrix += su`.
    pub fn add_source(&mut self, fv: &FvGeometry, su: &[f64]) {
        for ((s, &v), &x) in self.ldu.source.iter_mut().zip(fv.volumes()).zip(su) {
            *s -= v * x;
        }
    }

    /// `fvm::Sp(sp, ψ)`: the diagonal gains `V·sp`.
    pub fn sp(mesh: &PolyMesh, fv: &FvGeometry, sp: &[f64]) -> Self {
        let mut m = Self::new(mesh);
        for ((d, &v), &s) in m.ldu.diag.iter_mut().zip(fv.volumes()).zip(sp) {
            *d += v * s;
        }
        m
    }

    /// `fvm::SuSp(sp, ψ)`: positive `sp` on the diagonal, negative `sp`
    /// explicitly with the current `psi`.
    pub fn su_sp(mesh: &PolyMesh, fv: &FvGeometry, sp: &[f64], psi: &[f64]) -> Self {
        let mut m = Self::new(mesh);
        for (((d, s), (&v, &x)), &p) in m
            .ldu
            .diag
            .iter_mut()
            .zip(m.ldu.source.iter_mut())
            .zip(fv.volumes().iter().zip(sp))
            .zip(psi)
        {
            *d += v * x.max(0.0);
            *s -= v * x.min(0.0) * p;
        }
        m
    }

    /// Implicit `∇·(φ ψ)` for the face flux `phi` (every face).
    pub fn convection(
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &Boundary,
        phi: &[f64],
        scheme: Interpolation,
    ) -> Result<Self> {
        let mut m = Self::new(mesh);
        crate::ops::convection_interior(&mut m.ldu, mesh, fv, phi, scheme)?;
        let n_internal = mesh.n_internal_faces();
        for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
            for (i, f) in patch.faces().enumerate() {
                let k = f.saturating_sub(n_internal);
                let flux = phi.get(f).copied().unwrap_or(0.0);
                // `valueInternalCoeffs` and `valueBoundaryCoeffs`.
                let (internal, boundary) = match cond {
                    ScalarBc::FixedValue(v) => (0.0, v.get(i).copied().unwrap_or(0.0)),
                    ScalarBc::ZeroGradient | ScalarBc::Reflect => (1.0, 0.0),
                    ScalarBc::FixedGradient(g) => {
                        let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let g = g.get(i).copied().unwrap_or(0.0);
                        (1.0, if dc > 0.0 { g / dc } else { 0.0 })
                    }
                    ScalarBc::Mixed {
                        fraction,
                        ref_value,
                        ref_gradient,
                    } => {
                        let frac = fraction.get(i).copied().unwrap_or(0.0);
                        let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let g = ref_gradient.get(i).copied().unwrap_or(0.0);
                        (
                            1.0 - frac,
                            frac * ref_value.get(i).copied().unwrap_or(0.0)
                                + if dc > 0.0 { (1.0 - frac) * g / dc } else { 0.0 },
                        )
                    }
                    ScalarBc::Empty => continue,
                    ScalarBc::Calculated(_) => return Err(calculated(&patch.name)),
                };
                if let Some(c) = m.internal_coeffs.get_mut(k) {
                    *c = flux * internal;
                }
                if let Some(c) = m.boundary_coeffs.get_mut(k) {
                    *c = -flux * boundary;
                }
            }
        }
        Ok(m)
    }

    /// The `bounded` variant's `− fvm::Sp(∇·φ, ψ)`.
    pub fn bound_convection(&mut self, mesh: &PolyMesh, fv: &FvGeometry, phi: &[f64]) {
        let div = surface_integrate(mesh, fv, phi);
        for ((d, &v), &s) in self.ldu.diag.iter_mut().zip(fv.volumes()).zip(&div) {
            *d -= v * s;
        }
    }

    /// Implicit `∇·(Γ∇ψ)` with `gamma` given on every face, OpenFOAM's sign
    /// (negative diagonal). Only the orthogonal and uncorrected gradients.
    pub fn laplacian(
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &Boundary,
        gamma: &[f64],
        sn_grad: SnGrad,
    ) -> Result<Self> {
        if sn_grad == SnGrad::Corrected {
            return Err(FvError::Setup(
                "the corrected Laplacian with a varying coefficient is not supported yet".into(),
            ));
        }
        let dc = sn_grad.coeffs(fv);
        let gm = |f: usize| {
            gamma.get(f).copied().unwrap_or(0.0) * fv.mag_sf.get(f).copied().unwrap_or(0.0)
        };
        let mut m = Self::new(mesh);
        let ldu = &mut m.ldu;
        for f in 0..mesh.n_internal_faces() {
            let (Some(&l), Some(&u)) = (ldu.l.get(f), ldu.u.get(f)) else {
                continue;
            };
            let coeff = dc.get(f).copied().unwrap_or(0.0) * gm(f);
            if let (Some(up), Some(lo)) = (ldu.upper.get_mut(f), ldu.lower.get_mut(f)) {
                *up = coeff;
                *lo = coeff;
            }
            if let Some(d) = ldu.diag.get_mut(l) {
                *d -= coeff;
            }
            if let Some(d) = ldu.diag.get_mut(u) {
                *d -= coeff;
            }
        }
        let n_internal = mesh.n_internal_faces();
        for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
            for (i, f) in patch.faces().enumerate() {
                let k = f.saturating_sub(n_internal);
                let d = dc.get(f).copied().unwrap_or(0.0);
                // `gradientInternalCoeffs` and `gradientBoundaryCoeffs`.
                let (internal, boundary) = match cond {
                    ScalarBc::FixedValue(v) => (-d, d * v.get(i).copied().unwrap_or(0.0)),
                    ScalarBc::FixedGradient(g) => (0.0, g.get(i).copied().unwrap_or(0.0)),
                    ScalarBc::ZeroGradient | ScalarBc::Reflect => (0.0, 0.0),
                    ScalarBc::Mixed {
                        fraction,
                        ref_value,
                        ref_gradient,
                    } => {
                        let frac = fraction.get(i).copied().unwrap_or(0.0);
                        let r = ref_value.get(i).copied().unwrap_or(0.0);
                        let g = ref_gradient.get(i).copied().unwrap_or(0.0);
                        (-frac * d, frac * d * r + (1.0 - frac) * g)
                    }
                    ScalarBc::Empty => continue,
                    ScalarBc::Calculated(_) => return Err(calculated(&patch.name)),
                };
                if let Some(c) = m.internal_coeffs.get_mut(k) {
                    *c = gm(f) * internal;
                }
                if let Some(c) = m.boundary_coeffs.get_mut(k) {
                    *c = -gm(f) * boundary;
                }
            }
        }
        Ok(m)
    }

    fn boundary_faces(mesh: &PolyMesh) -> impl Iterator<Item = (usize, usize)> + '_ {
        let n_internal = mesh.n_internal_faces();
        mesh.patches()
            .iter()
            .filter(|p| p.kind != PatchKind::Empty)
            .flat_map(|p| p.faces())
            .filter_map(move |f| mesh.owner().get(f).map(|&c| (c, f - n_internal)))
    }

    /// Implicit under-relaxation by `alpha`, as `fvMatrix::relax`.
    pub fn relax(&mut self, mesh: &PolyMesh, psi: &[f64], alpha: f64) {
        if alpha <= 0.0 {
            return;
        }
        let d0 = self.ldu.diag.clone();
        let mut sum_off = vec![0.0; d0.len()];
        for (f, (&l, &u)) in self.ldu.l.iter().zip(&self.ldu.u).enumerate() {
            if let (Some(s), Some(&lo)) = (sum_off.get_mut(u), self.ldu.lower.get(f)) {
                *s += lo.abs();
            }
            if let (Some(s), Some(&up)) = (sum_off.get_mut(l), self.ldu.upper.get(f)) {
                *s += up.abs();
            }
        }
        for (c, k) in Self::boundary_faces(mesh) {
            if let (Some(d), Some(ic)) = (self.ldu.diag.get_mut(c), self.internal_coeffs.get(k)) {
                *d += ic.abs();
            }
        }
        for (d, &off) in self.ldu.diag.iter_mut().zip(&sum_off) {
            *d = d.abs().max(off);
            *d /= alpha;
        }
        for (c, k) in Self::boundary_faces(mesh) {
            if let (Some(d), Some(ic)) = (self.ldu.diag.get_mut(c), self.internal_coeffs.get(k)) {
                *d -= ic;
            }
        }
        for ((s, (&d, &d0)), &p) in self
            .ldu
            .source
            .iter_mut()
            .zip(self.ldu.diag.iter().zip(&d0))
            .zip(psi)
        {
            *s += (d - d0) * p;
        }
    }

    /// Fixes `psi` in `cells` to `values`, as `fvMatrix::setValues`: the
    /// row becomes `diag·ψ = diag·value`, and the cell's couplings move to
    /// its neighbours' sources.
    pub fn set_values(
        &mut self,
        mesh: &PolyMesh,
        cells: &[usize],
        values: &[f64],
        psi: &mut [f64],
    ) {
        let n_internal = mesh.n_internal_faces();
        let symmetric = self.ldu.is_symmetric();
        let mut faces_of: Vec<Vec<usize>> = Vec::new();
        if !cells.is_empty() {
            faces_of = vec![Vec::new(); mesh.n_cells()];
            for (f, &o) in mesh.owner().iter().enumerate() {
                if let Some(v) = faces_of.get_mut(o) {
                    v.push(f);
                }
            }
            for (f, &n) in mesh.neighbour().iter().enumerate() {
                if let Some(v) = faces_of.get_mut(n) {
                    v.push(f);
                }
            }
            for v in &mut faces_of {
                v.sort_unstable();
            }
        }
        let empty = |f: usize| {
            mesh.patches()
                .iter()
                .any(|p| p.kind == PatchKind::Empty && p.faces().contains(&f))
        };
        for (&c, &value) in cells.iter().zip(values) {
            if let Some(p) = psi.get_mut(c) {
                *p = value;
            }
            let diag = self.ldu.diag.get(c).copied().unwrap_or(0.0);
            if let Some(s) = self.ldu.source.get_mut(c) {
                *s = value * diag;
            }
            for &f in faces_of.get(c).map(Vec::as_slice).unwrap_or_default() {
                if f < n_internal {
                    let (Some(&o), Some(&n)) = (mesh.owner().get(f), mesh.neighbour().get(f))
                    else {
                        continue;
                    };
                    let up = self.ldu.upper.get(f).copied().unwrap_or(0.0);
                    let lo = self.ldu.lower.get(f).copied().unwrap_or(0.0);
                    let (other, coeff) = if c == o {
                        (n, if symmetric { up } else { lo })
                    } else {
                        (o, up)
                    };
                    if let Some(s) = self.ldu.source.get_mut(other) {
                        *s -= coeff * value;
                    }
                    if let Some(x) = self.ldu.upper.get_mut(f) {
                        *x = 0.0;
                    }
                    if let Some(x) = self.ldu.lower.get_mut(f) {
                        *x = 0.0;
                    }
                } else if !empty(f) {
                    let k = f - n_internal;
                    if let Some(x) = self.internal_coeffs.get_mut(k) {
                        *x = 0.0;
                    }
                    if let Some(x) = self.boundary_coeffs.get_mut(k) {
                        *x = 0.0;
                    }
                }
            }
        }
    }

    /// Solves for `psi` with the boundary coefficients added, as
    /// `fvMatrix::solveSegregated` for a scalar.
    pub fn solve<E: From<FvError>>(
        &self,
        mesh: &PolyMesh,
        psi: &mut [f64],
        solve: impl FnOnce(&LduMatrix, &mut [f64]) -> std::result::Result<Performance, E>,
    ) -> std::result::Result<Performance, E> {
        if psi.len() != self.ldu.n_cells() {
            return Err(FvError::Shape(format!(
                "{} unknowns for {} rows",
                psi.len(),
                self.ldu.n_cells()
            ))
            .into());
        }
        let mut m = self.ldu.clone();
        for (c, k) in Self::boundary_faces(mesh) {
            if let (Some(d), Some(&ic)) = (m.diag.get_mut(c), self.internal_coeffs.get(k)) {
                *d += ic;
            }
        }
        for (c, k) in Self::boundary_faces(mesh) {
            if let (Some(s), Some(&bc)) = (m.source.get_mut(c), self.boundary_coeffs.get(k)) {
                *s += bc;
            }
        }
        solve(&m, psi)
    }
}

fn calculated(patch: &str) -> FvError {
    FvError::Condition(format!(
        "patch '{patch}': a 'calculated' condition cannot be solved for"
    ))
}

#[cfg(test)]
mod tests {
    use vortia_core::{Aabb, Vec3};
    use vortia_mesh::blocks::structured_box;

    use super::*;

    /// Three cells in a row, x from 0 to 3, each of volume 1.
    fn row() -> (PolyMesh, FvGeometry) {
        let bounds = Aabb {
            min: Vec3::default(),
            max: Vec3::new(3.0, 1.0, 1.0),
        };
        let mesh = structured_box(bounds, [3, 1, 1]).unwrap();
        let fv = FvGeometry::new(&mesh);
        (mesh, fv)
    }

    /// A diffusion-like equation: −1 between neighbours, a diagonal of 3,
    /// and a coefficient on every boundary face.
    fn equation(mesh: &PolyMesh) -> ScalarMatrix {
        let mut m = ScalarMatrix::new(mesh);
        m.ldu.diag.fill(3.0);
        m.ldu.upper.fill(-1.0);
        m.ldu.lower.fill(-1.0);
        m.ldu.source = vec![1.0, 2.0, 3.0];
        m.internal_coeffs.fill(0.5);
        m.boundary_coeffs.fill(0.25);
        m
    }

    #[test]
    fn su_sp_is_implicit_when_positive_and_explicit_when_negative() {
        let (mesh, fv) = row();
        let m = ScalarMatrix::su_sp(&mesh, &fv, &[2.0, -2.0, 0.0], &[1.0, 5.0, 7.0]);
        assert_eq!(m.ldu.diag, [2.0, 0.0, 0.0]);
        assert_eq!(m.ldu.source, [0.0, 10.0, 0.0]);
        let sp = ScalarMatrix::sp(&mesh, &fv, &[2.0, -2.0, 0.0]);
        assert_eq!(sp.ldu.diag, [2.0, -2.0, 0.0]);
    }

    #[test]
    fn relaxation_scales_the_diagonal_and_keeps_the_solution() {
        let (mesh, _) = row();
        let psi = [1.0, 2.0, 3.0];
        let mut m = equation(&mesh);
        m.relax(&mesh, &psi, 1.0);
        // Diagonally dominant with α = 1: nothing moves.
        assert_eq!(m.ldu.diag, [3.0; 3]);
        assert_eq!(m.ldu.source, [1.0, 2.0, 3.0]);
        let mut m = equation(&mesh);
        m.relax(&mesh, &psi, 0.5);
        // Each cell's boundary coefficients join the diagonal before the
        // division and leave it after: an end cell has 5 boundary faces, the
        // middle one 4.
        let ends = (3.0 + 2.5) / 0.5 - 2.5;
        let middle = (3.0 + 2.0) / 0.5 - 2.0;
        assert_eq!(m.ldu.diag, [ends, middle, ends]);
        let source: Vec<f64> = [1.0, 2.0, 3.0]
            .iter()
            .zip([ends, middle, ends])
            .zip(psi)
            .map(|((s, d), p)| s + (d - 3.0) * p)
            .collect();
        assert_eq!(m.ldu.source, source);
        // α ≤ 0 is no relaxation at all.
        let mut m = equation(&mesh);
        m.relax(&mesh, &psi, 0.0);
        assert_eq!(m, equation(&mesh));
    }

    #[test]
    fn fixing_a_cell_moves_its_couplings_to_the_neighbours() {
        let (mesh, _) = row();
        let mut m = equation(&mesh);
        let mut psi = [0.0; 3];
        m.set_values(&mesh, &[1], &[10.0], &mut psi);
        assert_eq!(psi[1], 10.0);
        assert_eq!(m.ldu.source[1], 30.0);
        // Both neighbours took −(−1)·10 into their sources.
        assert_eq!(m.ldu.source[0], 11.0);
        assert_eq!(m.ldu.source[2], 13.0);
        assert!(m.ldu.upper.iter().all(|&x| x == 0.0));
        assert!(m.ldu.lower.iter().all(|&x| x == 0.0));
        // The middle cell's own boundary faces lose their coefficients; the
        // end cells keep theirs.
        let n_internal = mesh.n_internal_faces();
        for (k, (&ic, &bc)) in m.internal_coeffs.iter().zip(&m.boundary_coeffs).enumerate() {
            let owner = mesh.owner()[n_internal + k];
            let expected = if owner == 1 { (0.0, 0.0) } else { (0.5, 0.25) };
            assert_eq!((ic, bc), expected, "boundary face {k}");
        }
        // Fixing two cells side by side leaves no coupling between them to
        // carry into either source.
        let mut m = equation(&mesh);
        m.set_values(&mesh, &[0, 1], &[4.0, 6.0], &mut psi);
        assert_eq!(m.ldu.source, [12.0, 18.0, 9.0]);
    }

    #[test]
    fn solving_adds_the_boundary_and_checks_the_size() {
        let (mesh, _) = row();
        let m = equation(&mesh);
        let mut seen = None;
        let perf = m.solve::<FvError>(&mesh, &mut [0.0; 3], |a, _| {
            seen = Some((a.diag.clone(), a.source.clone()));
            Ok(Performance {
                solver: "test",
                initial_residual: 0.0,
                final_residual: 0.0,
                iterations: 0,
                converged: true,
            })
        });
        assert!(perf.is_ok());
        let (diag, source) = seen.unwrap();
        assert_eq!(diag, [5.5, 5.0, 5.5]);
        assert_eq!(source, [2.25, 3.0, 4.25]);
        let mut called = false;
        let short = m.solve::<FvError>(&mesh, &mut [0.0; 2], |_, _| {
            called = true;
            Err(FvError::Shape("solver reached".into()))
        });
        assert!(short.is_err());
        assert!(!called, "a short field must be refused before solving");
    }
}
