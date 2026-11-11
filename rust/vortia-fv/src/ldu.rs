//! Sparse matrices in lower-diagonal-upper form, addressed by the mesh's faces.
//!
//! Derived from OpenFOAM's `lduMatrix` (OpenFOAM, Copyright (C) 2011-2017
//! OpenFOAM Foundation and OpenCFD Ltd., GPL-3.0-or-later). Row `c` is cell
//! `c`; internal face `f` couples its owner `l = owner[f]` and neighbour
//! `u = neighbour[f]`, so `upper[f]` sits at `(l, u)` and `lower[f]` at
//! `(u, l)`. Because faces are in upper-triangular order, sweeping them
//! forward visits the matrix row by row, which is what the preconditioners and
//! smoothers rely on.

use vortia_mesh::PolyMesh;

/// A matrix and the right-hand side of `A x = source`.
#[derive(Debug, Clone, PartialEq)]
pub struct LduMatrix {
    pub diag: Vec<f64>,
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
    pub source: Vec<f64>,
    /// Owner of each internal face.
    pub l: Vec<usize>,
    /// Neighbour of each internal face.
    pub u: Vec<usize>,
}

impl LduMatrix {
    /// An all-zero matrix with the shape of `mesh`.
    pub fn new(mesh: &PolyMesh) -> Self {
        let n = mesh.n_cells();
        let nf = mesh.n_internal_faces();
        Self {
            diag: vec![0.0; n],
            upper: vec![0.0; nf],
            lower: vec![0.0; nf],
            source: vec![0.0; n],
            l: mesh.owner().get(..nf).unwrap_or_default().to_vec(),
            u: mesh.neighbour().to_vec(),
        }
    }

    pub fn n_cells(&self) -> usize {
        self.diag.len()
    }

    /// Whether `lower` equals `upper`, i.e. the matrix is symmetric.
    pub fn is_symmetric(&self) -> bool {
        self.lower == self.upper
    }

    fn faces(&self) -> impl Iterator<Item = (usize, usize, f64, f64)> + '_ {
        self.l
            .iter()
            .zip(&self.u)
            .zip(self.upper.iter().zip(&self.lower))
            .map(|((&l, &u), (&up, &lo))| (l, u, up, lo))
    }

    /// `y = A x`.
    pub fn amul(&self, x: &[f64], y: &mut [f64]) {
        for ((yc, &d), &xc) in y.iter_mut().zip(&self.diag).zip(x) {
            *yc = d * xc;
        }
        for (l, u, up, lo) in self.faces() {
            let (Some(&xl), Some(&xu)) = (x.get(l), x.get(u)) else {
                continue;
            };
            if let Some(yl) = y.get_mut(l) {
                *yl += up * xu;
            }
            if let Some(yu) = y.get_mut(u) {
                *yu += lo * xl;
            }
        }
    }

    /// `source − A x`, accumulated in the order OpenFOAM's
    /// `lduMatrix::residual` uses, so iteration counts agree.
    pub fn residual(&self, x: &[f64]) -> Vec<f64> {
        let mut r: Vec<f64> = self
            .source
            .iter()
            .zip(&self.diag)
            .zip(x)
            .map(|((&b, &d), &xc)| b - d * xc)
            .collect();
        for (l, u, up, lo) in self.faces() {
            let (Some(&xl), Some(&xu)) = (x.get(l), x.get(u)) else {
                continue;
            };
            if let Some(ru) = r.get_mut(u) {
                *ru -= lo * xl;
            }
            if let Some(rl) = r.get_mut(l) {
                *rl -= up * xu;
            }
        }
        r
    }

    /// Row sums.
    pub fn sum_a(&self) -> Vec<f64> {
        let mut s = self.diag.clone();
        for (l, u, up, lo) in self.faces() {
            if let Some(v) = s.get_mut(u) {
                *v += lo;
            }
            if let Some(v) = s.get_mut(l) {
                *v += up;
            }
        }
        s
    }

    /// The scale OpenFOAM divides residuals by, so they read the same
    /// whatever the size and units of the problem:
    /// `Σ |A x − A x̄| + |b − A x̄| + 1e-20`, with `x̄` the mean of `x`.
    pub fn norm_factor(&self, x: &[f64], ax: &[f64]) -> f64 {
        let mean = if x.is_empty() {
            0.0
        } else {
            x.iter().sum::<f64>() / x.len() as f64
        };
        let sum_a = self.sum_a();
        let mut total = 0.0;
        for ((&axc, &b), &sa) in ax.iter().zip(&self.source).zip(&sum_a) {
            let a_mean = sa * mean;
            total += (axc - a_mean).abs() + (b - a_mean).abs();
        }
        total + 1.0e-20
    }
}
