//! The face coefficients the discretisation needs on top of the raw geometry.
//!
//! Derived from OpenFOAM's `surfaceInterpolation` (`makeWeights`,
//! `makeDeltaCoeffs`, `makeNonOrthDeltaCoeffs`,
//! `makeNonOrthCorrectionVectors`; OpenFOAM, Copyright (C) 2011-2017 OpenFOAM
//! Foundation and OpenCFD Ltd., GPL-3.0-or-later).

use vortia_core::Vec3;
use vortia_mesh::{MeshGeometry, PolyMesh};

#[derive(Debug, Clone, PartialEq)]
pub struct FvGeometry {
    pub geo: MeshGeometry,
    /// |S_f| for every face.
    pub mag_sf: Vec<f64>,
    /// Linear interpolation weight of the owner, per internal face.
    pub weights: Vec<f64>,
    /// 1/max(n·d, 0.05|d|) for every face, with d from the owner centre to
    /// the neighbour centre (internal) or to the face centre (boundary): the
    /// coefficient of the orthogonal part of the face-normal gradient. Measured
    /// against OpenFOAM v1912, boundary conditions use this one too.
    pub non_orth_delta_coeffs: Vec<f64>,
    /// n − d·nonOrthDeltaCoeff per internal face: the direction of the
    /// explicit non-orthogonal correction.
    pub corr_vecs: Vec<Vec3>,
}

impl FvGeometry {
    pub fn new(mesh: &PolyMesh) -> Self {
        let geo = MeshGeometry::compute(mesh);
        let n_faces = mesh.n_faces();
        let n_internal = mesh.n_internal_faces();
        let own = mesh.owner();
        let nei = mesh.neighbour();

        let mut mag_sf = Vec::with_capacity(n_faces);
        let mut weights = Vec::with_capacity(n_internal);
        let mut non_orth_delta_coeffs = Vec::with_capacity(n_faces);
        let mut corr_vecs = Vec::with_capacity(n_internal);

        let centre = |c: usize| geo.cell_centres.get(c).copied().unwrap_or_default();
        for f in 0..n_faces {
            let sf = geo.face_areas.get(f).copied().unwrap_or_default();
            let cf = geo.face_centres.get(f).copied().unwrap_or_default();
            let c_own = own.get(f).map(|&o| centre(o)).unwrap_or_default();
            let mag = sf.norm();
            let unit = if mag > 0.0 { sf / mag } else { Vec3::default() };
            mag_sf.push(mag);

            let delta = match nei.get(f) {
                Some(&n) => {
                    let c_nei = centre(n);
                    let sfd_own = sf.dot(cf - c_own).abs();
                    let sfd_nei = sf.dot(c_nei - cf).abs();
                    let sum = sfd_own + sfd_nei;
                    weights.push(if sum > 0.0 { sfd_nei / sum } else { 0.5 });
                    c_nei - c_own
                }
                None => cf - c_own,
            };
            let mag_d = delta.norm();
            let denom = unit.dot(delta).max(0.05 * mag_d);
            let nodc = if denom > 0.0 { 1.0 / denom } else { 0.0 };
            non_orth_delta_coeffs.push(nodc);
            if f < n_internal {
                corr_vecs.push(unit - delta * nodc);
            }
        }

        Self {
            geo,
            mag_sf,
            weights,
            non_orth_delta_coeffs,
            corr_vecs,
        }
    }

    pub fn volumes(&self) -> &[f64] {
        &self.geo.cell_volumes
    }
}
