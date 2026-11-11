//! Iterative solvers for [`LduMatrix`] systems.
//!
//! Derived from OpenFOAM's `PCG`, `DICPreconditioner` and the convergence
//! rules of `lduMatrix::solver` (OpenFOAM, Copyright (C) 2011-2017 OpenFOAM
//! Foundation and OpenCFD Ltd., GPL-3.0-or-later). The residual is defined as
//! OpenFOAM defines it, so the two print the same numbers for the same system.

use crate::error::{FvError, Result};
use crate::ldu::LduMatrix;

/// When to stop, as `tolerance`, `relTol`, `maxIter` and `minIter` in
/// `fvSolution`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controls {
    pub tolerance: f64,
    pub rel_tol: f64,
    pub max_iter: usize,
    pub min_iter: usize,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            tolerance: 1e-6,
            rel_tol: 0.0,
            max_iter: 1000,
            min_iter: 0,
        }
    }
}

impl Controls {
    fn converged(&self, initial: f64, current: f64) -> bool {
        current < self.tolerance
            || (self.rel_tol > f64::EPSILON && current < self.rel_tol * initial)
    }
}

/// What a solve did, as OpenFOAM logs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Performance {
    pub solver: &'static str,
    pub initial_residual: f64,
    pub final_residual: f64,
    pub iterations: usize,
    pub converged: bool,
}

impl std::fmt::Display for Performance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:  Initial residual = {:e}, Final residual = {:e}, No Iterations {}",
            self.solver, self.initial_residual, self.final_residual, self.iterations
        )
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn sum_mag(a: &[f64]) -> f64 {
    a.iter().map(|v| v.abs()).sum()
}

/// The diagonal-based incomplete Cholesky preconditioner.
struct Dic {
    r_d: Vec<f64>,
}

impl Dic {
    fn new(m: &LduMatrix) -> Result<Self> {
        let mut r_d = m.diag.clone();
        for ((&l, &u), &up) in m.l.iter().zip(&m.u).zip(&m.upper) {
            let dl = r_d.get(l).copied().unwrap_or(0.0);
            if let Some(du) = r_d.get_mut(u) {
                *du -= up * up / dl;
            }
        }
        for d in &mut r_d {
            if *d == 0.0 || !d.is_finite() {
                return Err(FvError::Singular("DIC: a zero pivot".into()));
            }
            *d = 1.0 / *d;
        }
        Ok(Self { r_d })
    }

    fn precondition(&self, m: &LduMatrix, w: &mut [f64], r: &[f64]) {
        for ((wc, &d), &rc) in w.iter_mut().zip(&self.r_d).zip(r) {
            *wc = d * rc;
        }
        for ((&l, &u), &up) in m.l.iter().zip(&m.u).zip(&m.upper) {
            let wl = w.get(l).copied().unwrap_or(0.0);
            let du = self.r_d.get(u).copied().unwrap_or(0.0);
            if let Some(wu) = w.get_mut(u) {
                *wu -= du * up * wl;
            }
        }
        for ((&l, &u), &up) in m.l.iter().zip(&m.u).zip(&m.upper).rev() {
            let wu = w.get(u).copied().unwrap_or(0.0);
            let dl = self.r_d.get(l).copied().unwrap_or(0.0);
            if let Some(wl) = w.get_mut(l) {
                *wl -= dl * up * wu;
            }
        }
    }
}

/// Preconditioned conjugate gradients with DIC, for symmetric matrices.
pub fn pcg_dic(m: &LduMatrix, x: &mut [f64], c: &Controls) -> Result<Performance> {
    let n = m.n_cells();
    if x.len() != n || m.source.len() != n {
        return Err(FvError::Shape(format!("{} unknowns for {n} rows", x.len())));
    }
    if !m.is_symmetric() {
        return Err(FvError::Shape("PCG needs a symmetric matrix".into()));
    }
    let mut perf = Performance {
        solver: "DICPCG",
        initial_residual: 0.0,
        final_residual: 0.0,
        iterations: 0,
        converged: false,
    };
    let mut w_a = vec![0.0; n];
    m.amul(x, &mut w_a);
    let mut r_a: Vec<f64> = m.source.iter().zip(&w_a).map(|(b, a)| b - a).collect();
    let norm = m.norm_factor(x, &w_a);
    perf.initial_residual = sum_mag(&r_a) / norm;
    perf.final_residual = perf.initial_residual;

    if c.min_iter > 0 || !c.converged(perf.initial_residual, perf.final_residual) {
        let dic = Dic::new(m)?;
        let mut p_a = vec![0.0; n];
        let mut w_ar_a = 0.0;
        loop {
            let w_ar_a_old = w_ar_a;
            dic.precondition(m, &mut w_a, &r_a);
            w_ar_a = dot(&w_a, &r_a);
            if perf.iterations == 0 {
                p_a.copy_from_slice(&w_a);
            } else {
                let beta = w_ar_a / w_ar_a_old;
                for (p, &w) in p_a.iter_mut().zip(&w_a) {
                    *p = w + beta * *p;
                }
            }
            m.amul(&p_a, &mut w_a);
            let w_ap_a = dot(&w_a, &p_a);
            // OpenFOAM's singularity test: stop rather than divide by ~0.
            if (w_ap_a / norm).abs() < 1e-300 {
                break;
            }
            let alpha = w_ar_a / w_ap_a;
            for ((xc, &p), (r, &w)) in x.iter_mut().zip(&p_a).zip(r_a.iter_mut().zip(&w_a)) {
                *xc += alpha * p;
                *r -= alpha * w;
            }
            perf.final_residual = sum_mag(&r_a) / norm;
            perf.iterations += 1;
            let done = c.converged(perf.initial_residual, perf.final_residual);
            if !((perf.iterations < c.max_iter && !done) || perf.iterations < c.min_iter) {
                break;
            }
        }
    }
    perf.converged = c.converged(perf.initial_residual, perf.final_residual);
    if !perf.final_residual.is_finite() {
        return Err(FvError::Diverged(format!("{perf}")));
    }
    Ok(perf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1D chain of `n` cells: tridiagonal (−1, 2, −1).
    fn chain(n: usize) -> LduMatrix {
        LduMatrix {
            diag: vec![2.0; n],
            upper: vec![-1.0; n - 1],
            lower: vec![-1.0; n - 1],
            source: vec![1.0; n],
            l: (0..n - 1).collect(),
            u: (1..n).collect(),
        }
    }

    #[test]
    fn pcg_solves_a_chain_to_the_exact_answer() {
        let n = 50;
        let m = chain(n);
        let mut x = vec![0.0; n];
        let c = Controls {
            tolerance: 1e-14,
            ..Controls::default()
        };
        let perf = pcg_dic(&m, &mut x, &c).unwrap();
        assert!(perf.converged, "{perf}");
        // x_i = (i+1)(n-i)/2 solves the chain with unit source.
        for (i, &v) in x.iter().enumerate() {
            let exact = ((i + 1) * (n - i)) as f64 / 2.0;
            assert!((v - exact).abs() < 1e-9 * exact, "{i}: {v} vs {exact}");
        }
    }

    #[test]
    fn an_already_converged_system_takes_no_iterations() {
        let m = chain(3);
        let mut x = vec![1.5, 2.0, 1.5];
        let perf = pcg_dic(&m, &mut x, &Controls::default()).unwrap();
        assert_eq!(perf.iterations, 0);
    }

    #[test]
    fn bad_shapes_and_zero_pivots_are_errors() {
        let m = chain(3);
        assert!(pcg_dic(&m, &mut [0.0; 2], &Controls::default()).is_err());
        let mut z = chain(3);
        z.diag = vec![0.0; 3];
        assert!(pcg_dic(&z, &mut [0.0; 3], &Controls::default()).is_err());
    }
}
