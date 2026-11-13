//! The explicit parts the SIMPLE-family momentum and pressure equations
//! share: the non-orthogonal correction of a `Gauss linear corrected`
//! Laplacian, and the gradient correction of a `linearUpwind` convection.
//! The matrix holds the implicit part; these go to the source.

use vortia_core::Vec3;
use vortia_fv::limited::GradScheme;
use vortia_fv::{Boundary, FvGeometry, VectorBoundary, correction, ops, stress};
use vortia_mesh::PolyMesh;

/// The velocity field a correction is taken from.
pub(crate) struct Velocity<'a> {
    pub bc: &'a VectorBoundary,
    pub cells: &'a [Vec3],
}

/// Removes the divergence of `γ |S_f| (k · ∇U)_f` from `source`, the
/// explicit half of `laplacian(γ, U)` under `Gauss linear corrected`.
pub(crate) fn subtract_vector_laplacian_correction(
    source: &mut [Vec3],
    (mesh, fv): (&PolyMesh, &FvGeometry),
    scheme: GradScheme,
    u: &Velocity,
    gamma_f: &[f64],
) {
    let (g, _) = stress::grad(scheme, mesh, fv, u.bc, u.cells);
    let corr = correction::sn_grad_correction_vector(mesh, fv, &g);
    let flux = correction::scale_by_gamma_mag_sf(fv, gamma_f, &corr);
    correction::subtract_divergence(source, mesh, &flux);
}

/// Removes the divergence of `φ (corr)_f` from `source`: what
/// `linearUpwind` adds to the upwind face values, as `fvm::div` does.
pub(crate) fn subtract_linear_upwind_correction(
    source: &mut [Vec3],
    (mesh, fv): (&PolyMesh, &FvGeometry),
    scheme: GradScheme,
    u: &Velocity,
    phi: &[f64],
) {
    let (g, _) = stress::grad(scheme, mesh, fv, u.bc, u.cells);
    let corr = stress::linear_upwind_correction(mesh, fv, phi, &g);
    let flux: Vec<Vec3> = corr.iter().zip(phi).map(|(&c, &x)| c * x).collect();
    correction::subtract_divergence(source, mesh, &flux);
}

/// The explicit face flux `γ |S_f| (k · ∇p)_f` of `laplacian(γ, p)` under
/// `Gauss linear corrected`, from the pressure the matrix is assembled with.
pub(crate) fn pressure_laplacian_correction(
    (mesh, fv): (&PolyMesh, &FvGeometry),
    p_bc: &Boundary,
    p: &[f64],
    gamma_f: &[f64],
) -> Vec<f64> {
    let g = ops::grad(mesh, fv, p_bc, p);
    let corr = correction::sn_grad_correction(mesh, fv, &g);
    correction::scale_by_gamma_mag_sf(fv, gamma_f, &corr)
}
