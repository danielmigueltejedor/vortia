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
            ["Gauss", "linear", "uncorrected"] | ["Gauss", "linear", "orthogonal"] => {
                Ok(Self::Uncorrected)
            }
            other => Err(FvError::Setup(format!(
                "laplacian scheme '{}' is not supported yet",
                other.join(" ")
            ))),
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
    for f in 0..n_internal {
        let (Some(&l), Some(&u)) = (m.l.get(f), m.u.get(f)) else {
            continue;
        };
        let coeff = gamma
            * fv.mag_sf.get(f).copied().unwrap_or(0.0)
            * fv.non_orth_delta_coeffs.get(f).copied().unwrap_or(0.0);
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
                    // The same 1/max(n·d, 0.05|d|) as the internal faces.
                    let k = gm * fv.non_orth_delta_coeffs.get(f).copied().unwrap_or(0.0);
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
    /// From a `divSchemes` entry such as `Gauss upwind` or `bounded Gauss linear`.
    pub fn from_scheme(words: &[&str]) -> Result<Self> {
        let words = match words {
            ["bounded", rest @ ..] => rest,
            all => all,
        };
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
    if phi.len() != mesh.n_faces() {
        return Err(FvError::Shape(format!(
            "{} face fluxes for {} faces",
            phi.len(),
            mesh.n_faces()
        )));
    }
    let n_internal = mesh.n_internal_faces();
    for f in 0..n_internal {
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
