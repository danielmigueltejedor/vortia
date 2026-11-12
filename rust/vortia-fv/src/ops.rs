//! Finite-volume operators on scalar fields.
//!
//! Derived from OpenFOAM's `gaussGrad`, `gaussLaplacianScheme` with the
//! `corrected` and `uncorrected` surface-normal gradients, `gaussConvectionScheme`
//! with `upwind` and `linear` interpolation, and `EulerDdtScheme`
//! (OpenFOAM, Copyright (C) 2011-2017 OpenFOAM Foundation and OpenCFD Ltd.,
//! GPL-3.0-or-later).
//!
//! Sign convention: [`diffusion`] builds `−∇·(Γ∇φ)`, the negated Laplacian,
//! so that diffusion matrices have a positive diagonal and are solved as
//! written. That is OpenFOAM's `-fvm::laplacian(Γ, φ)`.

use vortia_core::Vec3;
use vortia_mesh::PolyMesh;

use crate::bc::{Boundary, ScalarBc};
use crate::error::{FvError, Result};
use crate::fvgeom::FvGeometry;
use crate::ldu::LduMatrix;

/// `Gauss linear` gradient of a cell field.
pub fn grad(mesh: &PolyMesh, fv: &FvGeometry, bc: &Boundary, cells: &[f64]) -> Vec<Vec3> {
    let n = mesh.n_cells();
    let mut g = vec![Vec3::default(); n];
    let own = mesh.owner();
    let nei = mesh.neighbour();
    let sf = &fv.geo.face_areas;
    for (f, (&o, &nb)) in own.iter().zip(nei).enumerate() {
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
    let boundary = bc.face_values(mesh, cells, &fv.non_orth_delta_coeffs);
    for (k, v) in boundary.into_iter().enumerate() {
        let f = mesh.n_internal_faces() + k;
        let (Some(v), Some(&s), Some(&o)) = (v, sf.get(f), own.get(f)) else {
            continue;
        };
        if let Some(go) = g.get_mut(o) {
            *go = *go + s * v;
        }
    }
    for (gc, &vol) in g.iter_mut().zip(fv.volumes()) {
        if vol > 0.0 {
            *gc = *gc / vol;
        }
    }
    g
}

/// How the face-normal gradient treats non-orthogonal faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnGrad {
    /// The difference of the two cells over their distance, as if every
    /// face were orthogonal.
    Orthogonal,
    /// The orthogonal part only.
    Uncorrected,
    /// Plus an explicit correction from the interpolated cell gradient.
    Corrected,
}

impl SnGrad {
    /// From a `laplacianSchemes` entry such as `Gauss linear corrected`.
    pub fn from_scheme(words: &[&str]) -> Result<Self> {
        match words {
            ["Gauss", "linear", "corrected"] => Ok(Self::Corrected),
            ["Gauss", "linear", "uncorrected"] => Ok(Self::Uncorrected),
            ["Gauss", "linear", "orthogonal"] => Ok(Self::Orthogonal),
            other => Err(FvError::Setup(format!(
                "laplacian scheme '{}' is not supported yet",
                other.join(" ")
            ))),
        }
    }
}

impl SnGrad {
    /// The face coefficients this gradient multiplies the cell difference by.
    pub fn coeffs(self, fv: &FvGeometry) -> &[f64] {
        match self {
            Self::Orthogonal => &fv.delta_coeffs,
            Self::Uncorrected | Self::Corrected => &fv.non_orth_delta_coeffs,
        }
    }
}

/// Adds `−∇·(Γ∇φ)` with constant `gamma` to `m`. `cells` is the current
/// solution, used by the explicit correction.
pub fn diffusion(
    m: &mut LduMatrix,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    gamma: f64,
    sn_grad: SnGrad,
    cells: &[f64],
) -> Result<()> {
    let n_internal = mesh.n_internal_faces();
    let dc = sn_grad.coeffs(fv);
    for f in 0..n_internal {
        let (Some(&l), Some(&u)) = (m.l.get(f), m.u.get(f)) else {
            continue;
        };
        let coeff =
            gamma * fv.mag_sf.get(f).copied().unwrap_or(0.0) * dc.get(f).copied().unwrap_or(0.0);
        if let (Some(up), Some(lo)) = (m.upper.get_mut(f), m.lower.get_mut(f)) {
            *up -= coeff;
            *lo -= coeff;
        }
        if let Some(d) = m.diag.get_mut(l) {
            *d += coeff;
        }
        if let Some(d) = m.diag.get_mut(u) {
            *d += coeff;
        }
    }

    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let Some(&c) = mesh.owner().get(f) else {
                continue;
            };
            let gm = gamma * fv.mag_sf.get(f).copied().unwrap_or(0.0);
            match cond {
                ScalarBc::FixedValue(v) => {
                    // The same coefficient as the internal faces.
                    let k = gm * dc.get(f).copied().unwrap_or(0.0);
                    if let (Some(d), Some(s)) = (m.diag.get_mut(c), m.source.get_mut(c)) {
                        *d += k;
                        *s += k * v.get(i).copied().unwrap_or(0.0);
                    }
                }
                ScalarBc::FixedGradient(g) => {
                    if let Some(s) = m.source.get_mut(c) {
                        *s += gm * g.get(i).copied().unwrap_or(0.0);
                    }
                }
                ScalarBc::ZeroGradient | ScalarBc::Reflect | ScalarBc::Empty => {}
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let k = gm
                        * dc.get(f).copied().unwrap_or(0.0)
                        * fraction.get(i).copied().unwrap_or(0.0);
                    if let (Some(d), Some(s)) = (m.diag.get_mut(c), m.source.get_mut(c)) {
                        *d += k;
                        *s += k * ref_value.get(i).copied().unwrap_or(0.0);
                    }
                }
                ScalarBc::Calculated(_) => {
                    return Err(FvError::Condition(format!(
                        "patch '{}': a 'calculated' condition cannot be solved for",
                        patch.name
                    )));
                }
            }
        }
    }

    if sn_grad == SnGrad::Corrected {
        let g = grad(mesh, fv, bc, cells);
        for f in 0..n_internal {
            let (Some(&l), Some(&u), Some(&w), Some(&cv)) = (
                m.l.get(f),
                m.u.get(f),
                fv.weights.get(f),
                fv.corr_vecs.get(f),
            ) else {
                continue;
            };
            let (Some(&gl), Some(&gu)) = (g.get(l), g.get(u)) else {
                continue;
            };
            let gf = gl * w + gu * (1.0 - w);
            let flux = gamma * fv.mag_sf.get(f).copied().unwrap_or(0.0) * cv.dot(gf);
            // −∇·(correction flux) moved to the right-hand side.
            if let Some(s) = m.source.get_mut(l) {
                *s += flux;
            }
            if let Some(s) = m.source.get_mut(u) {
                *s -= flux;
            }
        }
    }
    Ok(())
}

/// How `Gauss <scheme>` interpolates the convected value to a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    /// The value of the cell the flux comes from.
    Upwind,
    /// The distance-weighted mean of the two cells.
    Linear,
}

impl Interpolation {
    /// From a `divSchemes` entry such as `Gauss upwind`. `bounded` changes
    /// the equation, not the interpolation: read it with [`Convection`].
    pub fn from_scheme(words: &[&str]) -> Result<Self> {
        match words {
            ["Gauss", "upwind"] => Ok(Self::Upwind),
            ["Gauss", "linear"] => Ok(Self::Linear),
            other => Err(FvError::Setup(format!(
                "div scheme '{}' is not supported yet",
                other.join(" ")
            ))),
        }
    }
}

/// A `divSchemes` entry: the interpolation, and whether it is `bounded`,
/// which takes `(∇·φ) ψ` back out so that a flux that does not conserve
/// (a steady solve before it converges) cannot change a uniform field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Convection {
    pub interpolation: Interpolation,
    pub bounded: bool,
}

impl Convection {
    /// From a `divSchemes` entry such as `bounded Gauss linear`.
    pub fn from_scheme(words: &[&str]) -> Result<Self> {
        let (bounded, rest) = match words {
            ["bounded", rest @ ..] => (true, rest),
            all => (false, all),
        };
        Ok(Self {
            interpolation: Interpolation::from_scheme(rest)?,
            bounded,
        })
    }
}

/// What `bounded` subtracts, `fvm::Sp(fvc::surfaceIntegrate(phi), ψ)`: the
/// cell's net outflow on the diagonal.
pub fn bounded_correction(m: &mut LduMatrix, mesh: &PolyMesh, fv: &FvGeometry, phi: &[f64]) {
    let div = surface_integrate(mesh, fv, phi);
    for ((d, &net), &v) in m.diag.iter_mut().zip(&div).zip(fv.volumes()) {
        *d -= v * net;
    }
}

/// The internal-face part of the implicit `∇·(φ ψ)`, shared by every
/// matrix type: it checks that `phi` has one flux per face.
pub(crate) fn convection_interior(
    m: &mut LduMatrix,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    phi: &[f64],
    scheme: Interpolation,
) -> Result<()> {
    if phi.len() != mesh.n_faces() {
        return Err(FvError::Shape(format!(
            "{} face fluxes for {} faces",
            phi.len(),
            mesh.n_faces()
        )));
    }
    for f in 0..mesh.n_internal_faces() {
        let (Some(&l), Some(&u), Some(&flux)) = (m.l.get(f), m.u.get(f), phi.get(f)) else {
            continue;
        };
        let w = match scheme {
            Interpolation::Upwind => {
                if flux >= 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Interpolation::Linear => fv.weights.get(f).copied().unwrap_or(0.5),
        };
        // The owner's share goes on the diagonal, the neighbour's off it;
        // OpenFOAM's `negSumDiag` makes each row sum to the net outflow.
        let lower = -w * flux;
        let upper = lower + flux;
        if let (Some(up), Some(lo)) = (m.upper.get_mut(f), m.lower.get_mut(f)) {
            *up += upper;
            *lo += lower;
        }
        if let Some(d) = m.diag.get_mut(l) {
            *d -= lower;
        }
        if let Some(d) = m.diag.get_mut(u) {
            *d -= upper;
        }
    }
    Ok(())
}

/// Adds the implicit `∇·(φ ψ)` for a face flux `phi` (internal faces, then
/// boundary faces in patch order).
pub fn convection(
    m: &mut LduMatrix,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    phi: &[f64],
    scheme: Interpolation,
) -> Result<()> {
    convection_interior(m, mesh, fv, phi, scheme)?;
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let (Some(&c), Some(&flux)) = (mesh.owner().get(f), phi.get(f)) else {
                continue;
            };
            // `valueInternalCoeffs` on the diagonal, `valueBoundaryCoeffs` in
            // the source, each times the face flux.
            let (internal, boundary) = match cond {
                ScalarBc::FixedValue(v) => (0.0, v.get(i).copied().unwrap_or(0.0)),
                ScalarBc::ZeroGradient | ScalarBc::Reflect => (1.0, 0.0),
                ScalarBc::FixedGradient(g) => {
                    let dc = fv.non_orth_delta_coeffs.get(f).copied().unwrap_or(0.0);
                    let g = g.get(i).copied().unwrap_or(0.0);
                    (1.0, if dc > 0.0 { g / dc } else { 0.0 })
                }
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let f = fraction.get(i).copied().unwrap_or(0.0);
                    (1.0 - f, f * ref_value.get(i).copied().unwrap_or(0.0))
                }
                ScalarBc::Empty => continue,
                ScalarBc::Calculated(_) => {
                    return Err(FvError::Condition(format!(
                        "patch '{}': a 'calculated' condition cannot be solved for",
                        patch.name
                    )));
                }
            };
            if let (Some(d), Some(s)) = (m.diag.get_mut(c), m.source.get_mut(c)) {
                *d += flux * internal;
                *s -= flux * boundary;
            }
        }
    }
    Ok(())
}

/// Adds the implicit Euler time derivative `∂φ/∂t` with step `dt`.
pub fn ddt_euler(m: &mut LduMatrix, fv: &FvGeometry, old: &[f64], dt: f64) -> Result<()> {
    if !(dt > 0.0 && dt.is_finite()) {
        return Err(FvError::Setup(format!("time step {dt} is not positive")));
    }
    for ((d, s), (&v, &o)) in m
        .diag
        .iter_mut()
        .zip(m.source.iter_mut())
        .zip(fv.volumes().iter().zip(old))
    {
        let k = v / dt;
        *d += k;
        *s += k * o;
    }
    Ok(())
}

/// `w·(P − N) + N` on every internal face, then the boundary values given
/// (`0` where `None`): OpenFOAM's linear `interpolate` of a scalar.
pub fn interpolate_linear(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    cells: &[f64],
    boundary: &[Option<f64>],
) -> Vec<f64> {
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or(0.0);
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let w = fv.weights.get(f).copied().unwrap_or(0.5);
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        out.push(w * (o - n) + n);
    }
    out.extend(boundary.iter().map(|v| v.unwrap_or(0.0)));
    out
}

/// `∇·φ` of a face flux given on every face: the net outflow of each cell
/// over its volume (OpenFOAM's `surfaceIntegrate`). Empty faces carry none.
pub fn surface_integrate(mesh: &PolyMesh, fv: &FvGeometry, phi: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; mesh.n_cells()];
    for (f, (&o, &n)) in mesh.owner().iter().zip(mesh.neighbour()).enumerate() {
        let flux = phi.get(f).copied().unwrap_or(0.0);
        if let Some(v) = out.get_mut(o) {
            *v += flux;
        }
        if let Some(v) = out.get_mut(n) {
            *v -= flux;
        }
    }
    for patch in mesh.patches() {
        if patch.kind == vortia_mesh::PatchKind::Empty {
            continue;
        }
        for f in patch.faces() {
            if let (Some(&o), Some(&flux)) = (mesh.owner().get(f), phi.get(f))
                && let Some(v) = out.get_mut(o)
            {
                *v += flux;
            }
        }
    }
    for (v, &vol) in out.iter_mut().zip(fv.volumes()) {
        *v /= vol;
    }
    out
}

/// Implicit `∇·(Γ∇φ)` with `gamma` given on every face, OpenFOAM's sign
/// (negative diagonal): `fvm::laplacian(Γ_f, φ)`. Only the orthogonal and
/// uncorrected gradients are supported.
pub fn laplacian(
    m: &mut LduMatrix,
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    gamma: &[f64],
    sn_grad: SnGrad,
) -> Result<()> {
    if sn_grad == SnGrad::Corrected {
        return Err(FvError::Setup(
            "the corrected Laplacian with a varying coefficient is not supported yet".into(),
        ));
    }
    let dc = sn_grad.coeffs(fv);
    let gm =
        |f: usize| gamma.get(f).copied().unwrap_or(0.0) * fv.mag_sf.get(f).copied().unwrap_or(0.0);
    for f in 0..mesh.n_internal_faces() {
        let (Some(&l), Some(&u)) = (m.l.get(f), m.u.get(f)) else {
            continue;
        };
        let coeff = dc.get(f).copied().unwrap_or(0.0) * gm(f);
        if let (Some(up), Some(lo)) = (m.upper.get_mut(f), m.lower.get_mut(f)) {
            *up += coeff;
            *lo += coeff;
        }
        if let Some(d) = m.diag.get_mut(l) {
            *d -= coeff;
        }
        if let Some(d) = m.diag.get_mut(u) {
            *d -= coeff;
        }
    }
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let Some(&c) = mesh.owner().get(f) else {
                continue;
            };
            let d = dc.get(f).copied().unwrap_or(0.0);
            // `internalCoeffs` on the diagonal, `boundaryCoeffs` in the source.
            let (internal, boundary) = match cond {
                ScalarBc::FixedValue(v) => {
                    let v = v.get(i).copied().unwrap_or(0.0);
                    (gm(f) * -d, -gm(f) * (d * v))
                }
                ScalarBc::FixedGradient(g) => (0.0, -gm(f) * g.get(i).copied().unwrap_or(0.0)),
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let f_m = fraction.get(i).copied().unwrap_or(0.0);
                    let r = ref_value.get(i).copied().unwrap_or(0.0);
                    (gm(f) * (-f_m * d), -gm(f) * (f_m * d * r))
                }
                ScalarBc::ZeroGradient | ScalarBc::Reflect | ScalarBc::Empty => continue,
                ScalarBc::Calculated(_) => {
                    return Err(FvError::Condition(format!(
                        "patch '{}': a 'calculated' condition cannot be solved for",
                        patch.name
                    )));
                }
            };
            if let (Some(dg), Some(s)) = (m.diag.get_mut(c), m.source.get_mut(c)) {
                *dg += internal;
                *s += boundary;
            }
        }
    }
    Ok(())
}

/// The face flux of a solved [`laplacian`]: `Γ_f |S_f| ∂φ/∂n` on every face
/// (OpenFOAM's `fvMatrix::flux`), zero on empty faces.
pub fn laplacian_flux(
    mesh: &PolyMesh,
    fv: &FvGeometry,
    bc: &Boundary,
    gamma: &[f64],
    sn_grad: SnGrad,
    cells: &[f64],
) -> Vec<f64> {
    let dc = sn_grad.coeffs(fv);
    let gm =
        |f: usize| gamma.get(f).copied().unwrap_or(0.0) * fv.mag_sf.get(f).copied().unwrap_or(0.0);
    let cell = |c: Option<&usize>| c.and_then(|&c| cells.get(c)).copied().unwrap_or(0.0);
    let mut out = Vec::with_capacity(mesh.n_faces());
    for f in 0..mesh.n_internal_faces() {
        let coeff = dc.get(f).copied().unwrap_or(0.0) * gm(f);
        let (o, n) = (cell(mesh.owner().get(f)), cell(mesh.neighbour().get(f)));
        out.push(coeff * n - coeff * o);
    }
    for (patch, cond) in mesh.patches().iter().zip(&bc.conditions) {
        for (i, f) in patch.faces().enumerate() {
            let d = dc.get(f).copied().unwrap_or(0.0);
            let inside = cell(mesh.owner().get(f));
            out.push(match cond {
                ScalarBc::FixedValue(v) => {
                    let v = v.get(i).copied().unwrap_or(0.0);
                    gm(f) * -d * inside - -gm(f) * (d * v)
                }
                ScalarBc::FixedGradient(g) => -(-gm(f) * g.get(i).copied().unwrap_or(0.0)),
                ScalarBc::Mixed {
                    fraction,
                    ref_value,
                } => {
                    let f_m = fraction.get(i).copied().unwrap_or(0.0);
                    let r = ref_value.get(i).copied().unwrap_or(0.0);
                    gm(f) * (-f_m * d) * inside - -gm(f) * (f_m * d * r)
                }
                _ => 0.0,
            });
        }
    }
    out
}

/// Pins the solution of a matrix with no fixed value at `cell` to `value`
/// (OpenFOAM's `setReference`).
pub fn set_reference(m: &mut LduMatrix, cell: usize, value: f64) {
    if let (Some(d), Some(s)) = (m.diag.get_mut(cell), m.source.get_mut(cell)) {
        *s += *d * value;
        *d += *d;
    }
}
