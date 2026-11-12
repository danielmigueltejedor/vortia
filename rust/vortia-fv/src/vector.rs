//! Vector fields: their boundary conditions, and matrices for an equation in
//! a vector such as the momentum equation.
//!
//! Derived from OpenFOAM's `fvMatrix` (`A`, `H`, `solveSegregated`), its
//! `gaussConvectionScheme`, `gaussLaplacianScheme` and `EulerDdtScheme`, and
//! the `fixedValue`, `noSlip`, `zeroGradient` and `empty` conditions
//! (OpenFOAM, Copyright (C) 2011-2017 OpenFOAM Foundation and OpenCFD Ltd.,
//! GPL-3.0-or-later).
//!
//! As in OpenFOAM, one scalar matrix serves all three components; what
//! differs per component lives in the source and in the boundary
//! coefficients, which are kept apart from the matrix so that `A` and `H`
//! (the pieces of the pressure–velocity coupling) can be formed from them.
//! A matrix here is OpenFOAM's sign: it represents `A ψ − source`, so a
//! Laplacian has a negative diagonal and enters an equation subtracted.

use vortia_core::Vec3;
use vortia_fields::{Data, VectorField};
use vortia_mesh::{MeshGeometry, PatchKind, PolyMesh};

use crate::bc::check_solvable;
use crate::error::{FvError, Result};
use crate::fvgeom::{FvGeometry, mat_mul, transform, wedge_face_rotation};
use crate::ldu::LduMatrix;
use crate::ops::{Interpolation, SnGrad};
use crate::solve::Performance;

#[derive(Debug, Clone, PartialEq)]
pub enum VectorBc {
    /// `fixedValue`, and `noSlip` as zero.
    FixedValue(Vec<Vec3>),
    ZeroGradient,
    /// The unused direction of a 2D case.
    Empty,
    /// OpenFOAM's `directionMixed` for a value fraction that is diagonal
    /// (`mask` holds 1 on the components that take `ref_value` and 0 on those
    /// with a zero gradient), as `pressureInletOutletVelocity` sets it where
    /// the flow comes in: the tangential part fixed, the normal part free.
    Mixed {
        mask: Vec<Vec3>,
        ref_value: Vec<Vec3>,
    },
    /// One side of an axisymmetric wedge: the face takes the cell's vector
    /// turned by `face_t` onto the side, and `cell_t` (the same turn twice)
    /// carries it to the other side. OpenFOAM's `wedge`, a `transform`
    /// condition.
    Wedge {
        face_t: [[f64; 3]; 3],
        cell_t: [[f64; 3]; 3],
    },
}

impl VectorBc {
    /// OpenFOAM's `snGradTransformDiag` for a wedge: the part of the cell
    /// value the transformed face value takes from the cell's own component,
    /// `0.5 (I − cellT)` on the diagonal.
    fn wedge_mask(cell_t: &[[f64; 3]; 3]) -> Vec3 {
        let [[xx, ..], [_, yy, _], [.., zz]] = *cell_t;
        Vec3::new(0.5 * (1.0 - xx), 0.5 * (1.0 - yy), 0.5 * (1.0 - zz))
    }

    /// A wedge's value on its face, from the cell value.
    fn wedge_face(face_t: &[[f64; 3]; 3], inside: Vec3) -> Vec3 {
        transform(face_t, inside)
    }

    /// A wedge's surface-normal gradient, `(cellT·ψ − ψ)·0.5·δ`.
    pub fn wedge_sn_grad(cell_t: &[[f64; 3]; 3], inside: Vec3, delta: f64) -> Vec3 {
        (transform(cell_t, inside) - inside) * (0.5 * delta)
    }
}

/// One vector condition per mesh patch, in patch order.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorBoundary {
    pub conditions: Vec<VectorBc>,
    /// Per patch: the solver sets the value from the flow, so that a
    /// correction of the velocity may still move it (OpenFOAM's
    /// `assignable`, as `pressureInletVelocity` answers it).
    pub assignable: Vec<bool>,
}

impl VectorBoundary {
    pub fn from_field(field: &VectorField, mesh: &PolyMesh) -> Result<Self> {
        Self::from_field_with(field, mesh, &[])
    }

    /// As [`from_field`](Self::from_field), also taking the conditions named
    /// in `driven`, whose values the solver sets from the flow:
    /// `pressureInletVelocity` starts as its file's `value` and
    /// `pressureInletOutletVelocity` as a zero gradient.
    pub fn from_field_with(field: &VectorField, mesh: &PolyMesh, driven: &[&str]) -> Result<Self> {
        check_solvable(field)?;
        field.check_against(mesh)?;
        let mut conditions = Vec::with_capacity(mesh.patches().len());
        let mut assignable = Vec::with_capacity(mesh.patches().len());
        let mut areas: Option<Vec<Vec3>> = None;
        for patch in mesh.patches() {
            let pf = field.patch_field(patch).ok_or_else(|| {
                FvError::Condition(format!("no condition for patch '{}'", patch.name))
            })?;
            let given = || {
                pf.value
                    .as_ref()
                    .and_then(|d: &Data<Vec3>| d.expand(patch.size))
                    .ok_or_else(|| {
                        FvError::Condition(format!(
                            "patch '{}': {} needs a 'value' of {} items",
                            patch.name, pf.kind, patch.size
                        ))
                    })
            };
            let bc = match pf.kind.as_str() {
                "fixedValue" => VectorBc::FixedValue(given()?),
                "noSlip" => VectorBc::FixedValue(vec![Vec3::default(); patch.size]),
                "zeroGradient" => VectorBc::ZeroGradient,
                "empty" => VectorBc::Empty,
                "wedge" => {
                    let areas = areas.get_or_insert_with(|| MeshGeometry::compute(mesh).face_areas);
                    let face_t = wedge_face_rotation(areas, patch);
                    VectorBc::Wedge {
                        face_t,
                        cell_t: mat_mul(&face_t, &face_t),
                    }
                }
                "pressureInletVelocity" if driven.contains(&"pressureInletVelocity") => {
                    VectorBc::FixedValue(given()?)
                }
                "pressureInletOutletVelocity"
                    if driven.contains(&"pressureInletOutletVelocity") =>
                {
                    if pf.settings.values("tangentialVelocity").is_some() {
                        return Err(FvError::Condition(format!(
                            "patch '{}': a tangentialVelocity is not supported yet",
                            patch.name
                        )));
                    }
                    VectorBc::Mixed {
                        mask: vec![Vec3::default(); patch.size],
                        ref_value: vec![Vec3::default(); patch.size],
                    }
                }
                other => {
                    return Err(FvError::Condition(format!(
                        "patch '{}': '{other}' is not supported for vectors yet",
                        patch.name
                    )));
                }
            };
            if (patch.kind == PatchKind::Empty) != (bc == VectorBc::Empty) {
                return Err(FvError::Condition(format!(
                    "patch '{}' is a {} patch but the field says {}",
                    patch.name,
                    patch.kind.name(),
                    pf.kind
                )));
            }
            assignable.push(driven.contains(&pf.kind.as_str()));
            conditions.push(bc);
        }
        Ok(Self {
            conditions,
            assignable,
        })
    }

    /// Whether the condition on patch `i` sets the value (OpenFOAM's
    /// `fixesValue`, and the opposite of `assignable`).
    pub fn fixes_value(&self, i: usize) -> bool {
        matches!(self.conditions.get(i), Some(VectorBc::FixedValue(_)))
            && !self.assignable.get(i).copied().unwrap_or(false)
    }

    /// The value on every boundary face given the cell values, `None` on
    /// empty patches.
    pub fn face_values(&self, mesh: &PolyMesh, cells: &[Vec3]) -> Vec<Option<Vec3>> {
        let mut out = Vec::with_capacity(mesh.n_faces().saturating_sub(mesh.n_internal_faces()));
        for (patch, bc) in mesh.patches().iter().zip(&self.conditions) {
            for (i, f) in patch.faces().enumerate() {
                out.push(match bc {
                    VectorBc::FixedValue(v) => v.get(i).copied(),
                    VectorBc::ZeroGradient => Some(inside(mesh, cells, f)),
                    VectorBc::Empty => None,
                    VectorBc::Wedge { face_t, .. } => {
                        Some(VectorBc::wedge_face(face_t, inside(mesh, cells, f)))
                    }
                    VectorBc::Mixed { mask, ref_value } => {
                        let pif = inside(mesh, cells, f);
                        let m = mask.get(i).copied().unwrap_or_default();
                        let r = ref_value.get(i).copied().unwrap_or_default();
                        Some(Vec3::new(
                            m.x * r.x + (1.0 - m.x) * pif.x,
                            m.y * r.y + (1.0 - m.y) * pif.y,
                            m.z * r.z + (1.0 - m.z) * pif.z,
                        ))
                    }
                });
            }
        }
        out
    }
}

fn f_owner(mesh: &PolyMesh, f: usize) -> usize {
    mesh.owner().get(f).copied().unwrap_or(usize::MAX)
}

fn inside<T: Copy + Default>(mesh: &PolyMesh, cells: &[T], f: usize) -> T {
    mesh.owner()
        .get(f)
        .and_then(|&c| cells.get(c))
        .copied()
        .unwrap_or_default()
}

/// Which components are solved for: all but the directions normal to the
/// mesh's empty patches (OpenFOAM's `solutionD`).
pub fn solution_directions(mesh: &PolyMesh, fv: &FvGeometry) -> [bool; 3] {
    let mut empty = Vec3::default();
    let mut n_empty = 0usize;
    for patch in mesh.patches() {
        if patch.kind != PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            let s = fv.geo.face_areas.get(f).copied().unwrap_or_default();
            empty = empty + Vec3::new(s.x.abs(), s.y.abs(), s.z.abs());
            n_empty += 1;
        }
    }
    if n_empty == 0 {
        return [true; 3];
    }
    let mag = empty.norm();
    if mag > 0.0 {
        empty = empty / mag;
    }
    [empty.x <= 1e-6, empty.y <= 1e-6, empty.z <= 1e-6]
}

fn cmpt_av(v: Vec3) -> f64 {
    (v.x + v.y + v.z) / 3.0
}

fn mul_cmpt(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}

/// An equation in a vector field: `A ψ − source` with per-component
/// boundary coefficients, one entry per boundary face.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorMatrix {
    pub ldu: LduMatrix,
    pub source: Vec<Vec3>,
    /// Added to the diagonal of each component (`internalCoeffs`).
    pub internal_coeffs: Vec<Vec3>,
    /// Added to the source (`boundaryCoeffs`).
    pub boundary_coeffs: Vec<Vec3>,
}

impl VectorMatrix {
    /// An all-zero equation on `mesh`.
    pub fn new(mesh: &PolyMesh) -> Self {
        let nb = mesh.n_faces().saturating_sub(mesh.n_internal_faces());
        Self {
            ldu: LduMatrix::new(mesh),
            source: vec![Vec3::default(); mesh.n_cells()],
            internal_coeffs: vec![Vec3::default(); nb],
            boundary_coeffs: vec![Vec3::default(); nb],
        }
    }

    fn combine(&mut self, other: &Self, sign: f64) {
        let add = |a: &mut [f64], b: &[f64]| {
            for (x, &y) in a.iter_mut().zip(b) {
                if sign > 0.0 {
                    *x += y;
                } else {
                    *x -= y;
                }
            }
        };
        let add3 = |a: &mut [Vec3], b: &[Vec3]| {
            for (x, &y) in a.iter_mut().zip(b) {
                *x = if sign > 0.0 { *x + y } else { *x - y };
            }
        };
        add(&mut self.ldu.diag, &other.ldu.diag);
        add(&mut self.ldu.upper, &other.ldu.upper);
        add(&mut self.ldu.lower, &other.ldu.lower);
        add3(&mut self.source, &other.source);
        add3(&mut self.internal_coeffs, &other.internal_coeffs);
        add3(&mut self.boundary_coeffs, &other.boundary_coeffs);
    }

    /// Implicit under-relaxation by `alpha`, as `fvMatrix::relax`: the
    /// diagonal is first made at least as large as the off-diagonals (the
    /// boundary counted at its largest component), divided by `alpha`, and
    /// what it gained is balanced in the source with `psi`.
    pub fn relax(&mut self, mesh: &PolyMesh, psi: &[Vec3], alpha: f64) {
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
        let n_internal = mesh.n_internal_faces();
        let boundary_faces = || {
            mesh.patches()
                .iter()
                .filter(|p| p.kind != PatchKind::Empty)
                .flat_map(|p| p.faces())
        };
        for f in boundary_faces() {
            let (Some(&c), Some(ic)) = (
                mesh.owner().get(f),
                self.internal_coeffs.get(f.saturating_sub(n_internal)),
            ) else {
                continue;
            };
            if let Some(d) = self.ldu.diag.get_mut(c) {
                *d += ic.x.abs().max(ic.y.abs()).max(ic.z.abs());
            }
        }
        for (d, &off) in self.ldu.diag.iter_mut().zip(&sum_off) {
            *d = d.abs().max(off);
            *d /= alpha;
        }
        for f in boundary_faces() {
            let (Some(&c), Some(ic)) = (
                mesh.owner().get(f),
                self.internal_coeffs.get(f.saturating_sub(n_internal)),
            ) else {
                continue;
            };
            if let Some(d) = self.ldu.diag.get_mut(c) {
                *d -= ic.x.min(ic.y).min(ic.z);
            }
        }
        for ((s, (&d, &d0)), &p) in self
            .source
            .iter_mut()
            .zip(self.ldu.diag.iter().zip(&d0))
            .zip(psi)
        {
            *s = *s + p * (d - d0);
        }
    }

    /// `self + other`, term by term.
    pub fn add(&mut self, other: &Self) {
        self.combine(other, 1.0);
    }

    /// `self − other`, term by term.
    pub fn sub(&mut self, other: &Self) {
        self.combine(other, -1.0);
    }

    /// Implicit Euler `∂ψ/∂t`.
    pub fn ddt_euler(mesh: &PolyMesh, fv: &FvGeometry, old: &[Vec3], dt: f64) -> Result<Self> {
        if !(dt > 0.0 && dt.is_finite()) {
            return Err(FvError::Setup(format!("time step {dt} is not positive")));
        }
        let r_dt = 1.0 / dt;
        let mut m = Self::new(mesh);
        for (((d, s), &v), &o) in m
            .ldu
            .diag
            .iter_mut()
            .zip(m.source.iter_mut())
            .zip(fv.volumes())
            .zip(old)
        {
            *d = r_dt * v;
            *s = o * r_dt * v;
        }
        Ok(m)
    }

    /// Implicit `∇·(φ ψ)` for the face flux `phi` (every face). `psi` is the
    /// field's cell values, which a wedge's explicit part reads.
    pub fn convection(
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &VectorBoundary,
        phi: &[f64],
        psi: &[Vec3],
        scheme: Interpolation,
    ) -> Result<Self> {
        let mut m = Self::new(mesh);
        crate::ops::convection_interior(&mut m.ldu, mesh, fv, phi, scheme)?;
        let n_internal = mesh.n_internal_faces();
        for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
            for (i, f) in patch.faces().enumerate() {
                let k = f.saturating_sub(n_internal);
                let flux = phi.get(f).copied().unwrap_or(0.0);
                let (internal, boundary) = match cond {
                    VectorBc::FixedValue(v) => (Vec3::default(), v.get(i).copied()),
                    VectorBc::ZeroGradient => (Vec3::new(1.0, 1.0, 1.0), Some(Vec3::default())),
                    VectorBc::Mixed { mask, ref_value } => {
                        let m = mask.get(i).copied().unwrap_or_default();
                        let r = ref_value.get(i).copied().unwrap_or_default();
                        (
                            Vec3::new(1.0 - m.x, 1.0 - m.y, 1.0 - m.z),
                            Some(mul_cmpt(m, r)),
                        )
                    }
                    VectorBc::Wedge { face_t, cell_t } => {
                        let pif = psi.get(f_owner(mesh, f)).copied().unwrap_or_default();
                        let mask = VectorBc::wedge_mask(cell_t);
                        let internal = Vec3::new(1.0 - mask.x, 1.0 - mask.y, 1.0 - mask.z);
                        let face = VectorBc::wedge_face(face_t, pif);
                        (internal, Some(face - mul_cmpt(internal, pif)))
                    }
                    VectorBc::Empty => continue,
                };
                if let Some(c) = m.internal_coeffs.get_mut(k) {
                    *c = internal * flux;
                }
                if let (Some(c), Some(b)) = (m.boundary_coeffs.get_mut(k), boundary) {
                    *c = b * -flux;
                }
            }
        }
        Ok(m)
    }

    /// Implicit `∇·(Γ∇ψ)` with `gamma` given on every face, OpenFOAM's sign
    /// (negative diagonal). Non-orthogonal correction is not supported here.
    pub fn laplacian(
        mesh: &PolyMesh,
        fv: &FvGeometry,
        bc: &VectorBoundary,
        gamma: &[f64],
        psi: &[Vec3],
        sn_grad: SnGrad,
    ) -> Result<Self> {
        let gamma = |f: usize| gamma.get(f).copied().unwrap_or(0.0);
        if sn_grad == SnGrad::Corrected {
            return Err(FvError::Setup(
                "the corrected Laplacian of a vector is not supported yet".into(),
            ));
        }
        let dc = sn_grad.coeffs(fv);
        let mut m = Self::new(mesh);
        let ldu = &mut m.ldu;
        for f in 0..mesh.n_internal_faces() {
            let (Some(&l), Some(&u)) = (ldu.l.get(f), ldu.u.get(f)) else {
                continue;
            };
            let gm = gamma(f) * fv.mag_sf.get(f).copied().unwrap_or(0.0);
            let coeff = dc.get(f).copied().unwrap_or(0.0) * gm;
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
                let gm = gamma(f) * fv.mag_sf.get(f).copied().unwrap_or(0.0);
                let d = dc.get(f).copied().unwrap_or(0.0);
                match cond {
                    VectorBc::FixedValue(v) => {
                        let value = v.get(i).copied().unwrap_or_default();
                        if let Some(c) = m.internal_coeffs.get_mut(k) {
                            let g = -d;
                            *c = Vec3::new(gm * g, gm * g, gm * g);
                        }
                        if let Some(c) = m.boundary_coeffs.get_mut(k) {
                            *c = (value * d) * -gm;
                        }
                    }
                    VectorBc::Mixed { mask, ref_value } => {
                        let mk = mask.get(i).copied().unwrap_or_default();
                        let r = ref_value.get(i).copied().unwrap_or_default();
                        if let Some(c) = m.internal_coeffs.get_mut(k) {
                            *c = mk * (-d * gm);
                        }
                        if let Some(c) = m.boundary_coeffs.get_mut(k) {
                            *c = mul_cmpt(mk, r) * (-d * gm);
                        }
                    }
                    VectorBc::Wedge { cell_t, .. } => {
                        let pif = psi.get(f_owner(mesh, f)).copied().unwrap_or_default();
                        let dc = fv.delta_coeffs.get(f).copied().unwrap_or(0.0);
                        let mask = VectorBc::wedge_mask(cell_t);
                        let internal = mask * -dc;
                        let sn = VectorBc::wedge_sn_grad(cell_t, pif, dc);
                        if let Some(c) = m.internal_coeffs.get_mut(k) {
                            *c = internal * gm;
                        }
                        if let Some(c) = m.boundary_coeffs.get_mut(k) {
                            *c = (sn - mul_cmpt(internal, pif)) * -gm;
                        }
                    }
                    VectorBc::ZeroGradient | VectorBc::Empty => {}
                }
            }
        }
        Ok(m)
    }

    /// Calls `f(cell, coefficient)` for every boundary face's coefficient.
    fn each_boundary(&self, mesh: &PolyMesh, coeffs: &[Vec3], mut f: impl FnMut(usize, Vec3)) {
        let n_internal = mesh.n_internal_faces();
        for patch in mesh.patches() {
            if patch.kind == PatchKind::Empty {
                continue;
            }
            for face in patch.faces() {
                let (Some(&c), Some(&v)) = (
                    mesh.owner().get(face),
                    coeffs.get(face.saturating_sub(n_internal)),
                ) else {
                    continue;
                };
                f(c, v);
            }
        }
    }

    /// The diagonal with the mean of the boundary coefficients (OpenFOAM's `D()`).
    pub fn d(&self, mesh: &PolyMesh) -> Vec<f64> {
        let mut d = self.ldu.diag.clone();
        self.each_boundary(mesh, &self.internal_coeffs, |c, v| {
            if let Some(x) = d.get_mut(c) {
                *x += cmpt_av(v);
            }
        });
        d
    }

    /// `A`: the diagonal per unit volume.
    pub fn a(&self, mesh: &PolyMesh, fv: &FvGeometry) -> Vec<f64> {
        self.d(mesh)
            .iter()
            .zip(fv.volumes())
            .map(|(&d, &v)| d / v)
            .collect()
    }

    /// `H(1)`: minus the off-diagonal coefficients summed per row, per unit
    /// volume (`fvMatrix::H1`), what SIMPLEC takes from `A`.
    pub fn h1(&self, fv: &FvGeometry) -> Vec<f64> {
        let ldu = &self.ldu;
        let mut h1 = vec![0.0; ldu.n_cells()];
        for (((&l, &u), &up), &lo) in ldu.l.iter().zip(&ldu.u).zip(&ldu.upper).zip(&ldu.lower) {
            if let Some(x) = h1.get_mut(u) {
                *x -= lo;
            }
            if let Some(x) = h1.get_mut(l) {
                *x -= up;
            }
        }
        for (x, &v) in h1.iter_mut().zip(fv.volumes()) {
            *x /= v;
        }
        h1
    }

    /// `H`: everything but the diagonal, applied to `psi`, per unit volume.
    /// Components outside `valid` are zero.
    pub fn h(&self, mesh: &PolyMesh, fv: &FvGeometry, psi: &[Vec3], valid: [bool; 3]) -> Vec<Vec3> {
        let n = psi.len();
        let mut boundary_diag = vec![Vec3::default(); n];
        self.each_boundary(mesh, &self.internal_coeffs, |c, v| {
            if let Some(x) = boundary_diag.get_mut(c) {
                *x = *x + v;
            }
        });
        for x in &mut boundary_diag {
            *x = *x * -1.0;
        }
        self.each_boundary(mesh, &self.internal_coeffs, |c, v| {
            if let Some(x) = boundary_diag.get_mut(c) {
                let a = cmpt_av(v);
                *x = *x + Vec3::new(a, a, a);
            }
        });
        let mut h: Vec<Vec3> = boundary_diag
            .iter()
            .zip(psi)
            .map(|(&b, &p)| mul_cmpt(b, p))
            .collect();

        let mut off = vec![Vec3::default(); n];
        let ldu = &self.ldu;
        for (((&l, &u), &up), &lo) in ldu.l.iter().zip(&ldu.u).zip(&ldu.upper).zip(&ldu.lower) {
            let (Some(&pl), Some(&pu)) = (psi.get(l), psi.get(u)) else {
                continue;
            };
            if let Some(x) = off.get_mut(u) {
                *x = *x - pl * lo;
            }
            if let Some(x) = off.get_mut(l) {
                *x = *x - pu * up;
            }
        }
        for ((x, &o), &s) in h.iter_mut().zip(&off).zip(&self.source) {
            *x = *x + (o + s);
        }
        self.each_boundary(mesh, &self.boundary_coeffs, |c, v| {
            if let Some(x) = h.get_mut(c) {
                *x = *x + v;
            }
        });
        for (x, &v) in h.iter_mut().zip(fv.volumes()) {
            *x = *x / v;
            for (k, ok) in valid.iter().enumerate() {
                if !ok {
                    x[k] = 0.0;
                }
            }
        }
        h
    }

    /// Solves each valid component in turn, as OpenFOAM's `solveSegregated`:
    /// `extra` (already times the cell volume) is added to the source first.
    /// `solve` is called with the component's matrix, its unknowns and the
    /// component index.
    pub fn solve_segregated<E: From<FvError>>(
        &self,
        mesh: &PolyMesh,
        psi: &mut [Vec3],
        extra: Option<&[Vec3]>,
        valid: [bool; 3],
        mut solve: impl FnMut(&LduMatrix, &mut [f64], usize) -> std::result::Result<Performance, E>,
    ) -> std::result::Result<Vec<(usize, Performance)>, E> {
        if psi.len() != self.source.len() {
            return Err(FvError::Shape(format!(
                "{} unknowns for {} rows",
                psi.len(),
                self.source.len()
            ))
            .into());
        }
        let mut source = self.source.clone();
        if let Some(extra) = extra {
            for (s, &e) in source.iter_mut().zip(extra) {
                *s = *s + e;
            }
        }
        self.each_boundary(mesh, &self.boundary_coeffs, |c, v| {
            if let Some(x) = source.get_mut(c) {
                *x = *x + v;
            }
        });
        let mut out = Vec::new();
        let mut m = self.ldu.clone();
        for (cmpt, ok) in valid.iter().enumerate() {
            if !ok {
                continue;
            }
            m.diag.clone_from(&self.ldu.diag);
            self.each_boundary(mesh, &self.internal_coeffs, |c, v| {
                if let Some(x) = m.diag.get_mut(c) {
                    *x += v[cmpt];
                }
            });
            m.source = source.iter().map(|s| s[cmpt]).collect();
            let mut x: Vec<f64> = psi.iter().map(|p| p[cmpt]).collect();
            let perf = solve(&m, &mut x, cmpt)?;
            for (p, v) in psi.iter_mut().zip(x) {
                p[cmpt] = v;
            }
            out.push((cmpt, perf));
        }
        Ok(out)
    }
}

/// `Sf & ψ_f` on every face, with `ψ_f = w·(P − N) + N` inside and the
/// boundary values given (zero on empty faces): OpenFOAM's `dotInterpolate`.
pub fn dot_interpolate(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[Vec3],
    boundary: &[Option<Vec3>],
) -> Vec<f64> {
    let sf = &fv.geo.face_areas;
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or_default();
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        out.push(sf.get(f).copied().unwrap_or_default().dot((o - n) * w + n));
    }
    for (k, v) in boundary.iter().enumerate() {
        let f = mesh.n_internal_faces() + k;
        out.push(match v {
            Some(v) => sf.get(f).copied().unwrap_or_default().dot(*v),
            None => 0.0,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nozzle's wedge mesh, and a boundary giving its two sides the
    /// wedge condition and everything else a zero gradient.
    fn wedge_case() -> (PolyMesh, FvGeometry, VectorBoundary) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/nozzle-practice-2026");
        let mesh = PolyMesh::read_case(dir).unwrap();
        let fv = FvGeometry::new(&mesh);
        let areas = &fv.geo.face_areas;
        let conditions = mesh
            .patches()
            .iter()
            .map(|p| match p.kind {
                PatchKind::Wedge => {
                    let face_t = wedge_face_rotation(areas, p);
                    VectorBc::Wedge {
                        face_t,
                        cell_t: mat_mul(&face_t, &face_t),
                    }
                }
                PatchKind::Empty => VectorBc::Empty,
                _ => VectorBc::ZeroGradient,
            })
            .collect::<Vec<_>>();
        let assignable = vec![false; conditions.len()];
        (
            mesh,
            fv,
            VectorBoundary {
                conditions,
                assignable,
            },
        )
    }

    /// A vector in the wedge's centre plane (no azimuthal part), which the
    /// side turns onto itself: it then lies in the side's plane, so it carries
    /// nothing through it.
    #[test]
    fn a_wedge_turns_a_vector_of_the_centre_plane_into_the_side() {
        let (mesh, fv, bc) = wedge_case();
        let cells = vec![Vec3::new(2.0, 3.0, 0.0); mesh.n_cells()];
        let faces = bc.face_values(&mesh, &cells);
        let n_internal = mesh.n_internal_faces();
        for name in ["front", "back"] {
            let patch = mesh.patch(name).unwrap();
            for f in patch.faces() {
                let v = faces[f - n_internal].unwrap();
                let s = fv.geo.face_areas[f];
                assert!(
                    (v.norm() - 13f64.sqrt()).abs() < 1e-12,
                    "turning keeps length"
                );
                assert!((v.x - 2.0).abs() < 1e-12, "{name}: {v:?}");
                // The case file has six digits per point, so its sides are flat to
                // about that.
                assert!(s.dot(v).abs() < 1e-5 * s.norm() * v.norm(), "{name}: {v:?}");
            }
        }
    }

    /// What the matrices hold for a wedge face is the face's flux and the
    /// face's gradient: `internal·ψ − boundary` recovers `φ ψ_f` and
    /// `Γ |S| ∂ψ/∂n`, as OpenFOAM's `transform` conditions give them.
    #[test]
    fn a_wedge_face_carries_its_value_and_gradient_in_the_matrices() {
        let (mesh, fv, bc) = wedge_case();
        let cells: Vec<Vec3> = (0..mesh.n_cells())
            .map(|c| Vec3::new(1.0 + c as f64 * 1e-3, 2.0 - c as f64 * 2e-3, 0.5))
            .collect();
        let phi = vec![1.0; mesh.n_faces()];
        let gamma = vec![2.0; mesh.n_faces()];
        let conv =
            VectorMatrix::convection(&mesh, &fv, &bc, &phi, &cells, Interpolation::Upwind).unwrap();
        let lap =
            VectorMatrix::laplacian(&mesh, &fv, &bc, &gamma, &cells, SnGrad::Uncorrected).unwrap();
        let faces = bc.face_values(&mesh, &cells);
        let n_internal = mesh.n_internal_faces();
        let mut seen = 0;
        for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
            let VectorBc::Wedge { cell_t, .. } = cond else {
                continue;
            };
            for f in patch.faces() {
                let k = f - n_internal;
                let pif = cells[mesh.owner()[f]];
                let got = mul_cmpt(conv.internal_coeffs[k], pif) - conv.boundary_coeffs[k];
                let want = faces[k].unwrap();
                assert!((got - want).norm() < 1e-12, "value: {got:?} vs {want:?}");

                let dc = fv.delta_coeffs[f];
                let sn = VectorBc::wedge_sn_grad(cell_t, pif, dc);
                let got = mul_cmpt(lap.internal_coeffs[k], pif) - lap.boundary_coeffs[k];
                let want = sn * (2.0 * fv.mag_sf[f]);
                assert!((got - want).norm() < 1e-12, "gradient: {got:?} vs {want:?}");
                seen += 1;
            }
        }
        assert!(seen > 0);
    }
}
