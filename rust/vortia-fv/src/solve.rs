//! Iterative solvers for [`LduMatrix`] systems.
//!
//! Derived from OpenFOAM's `PCG`, `PBiCGStab`, `DICPreconditioner`,
//! `DILUPreconditioner` and the convergence
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

/// The diagonal-based incomplete LU preconditioner, for asymmetric matrices.
struct Dilu {
    r_d: Vec<f64>,
    /// Internal faces in neighbour order (OpenFOAM's `losort`): the forward
    /// sweep visits them so, which fixes the order round-off accumulates in.
    losort: Vec<usize>,
}

impl Dilu {
    fn new(m: &LduMatrix) -> Result<Self> {
        let mut r_d = m.diag.clone();
        for (((&l, &u), &up), &lo) in m.l.iter().zip(&m.u).zip(&m.upper).zip(&m.lower) {
            let dl = r_d.get(l).copied().unwrap_or(0.0);
            if let Some(du) = r_d.get_mut(u) {
                *du -= up * lo / dl;
            }
        }
        for d in &mut r_d {
            if *d == 0.0 || !d.is_finite() {
                return Err(FvError::Singular("DILU: a zero pivot".into()));
            }
            *d = 1.0 / *d;
        }
        let mut losort: Vec<usize> = (0..m.u.len()).collect();
        losort.sort_by_key(|&f| m.u.get(f).copied().unwrap_or(0));
        Ok(Self { r_d, losort })
    }

    fn precondition(&self, m: &LduMatrix, w: &mut [f64], r: &[f64]) {
        for ((wc, &d), &rc) in w.iter_mut().zip(&self.r_d).zip(r) {
            *wc = d * rc;
        }
        for &f in &self.losort {
            let (Some(&l), Some(&u), Some(&lo)) = (m.l.get(f), m.u.get(f), m.lower.get(f)) else {
                continue;
            };
            let wl = w.get(l).copied().unwrap_or(0.0);
            let du = self.r_d.get(u).copied().unwrap_or(0.0);
            if let Some(wu) = w.get_mut(u) {
                *wu -= du * lo * wl;
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

/// OpenFOAM's test for a quantity too small to divide by.
fn singular(v: f64) -> bool {
    v.abs() < 1e-300
}

/// Preconditioned bi-conjugate gradients, stabilised, with DILU: for
/// asymmetric matrices such as convection.
pub fn pbicgstab_dilu(m: &LduMatrix, x: &mut [f64], c: &Controls) -> Result<Performance> {
    let n = m.n_cells();
    if x.len() != n || m.source.len() != n {
        return Err(FvError::Shape(format!("{} unknowns for {n} rows", x.len())));
    }
    let mut perf = Performance {
        solver: "DILUPBiCGStab",
        initial_residual: 0.0,
        final_residual: 0.0,
        iterations: 0,
        converged: false,
    };
    let mut y_a = vec![0.0; n];
    m.amul(x, &mut y_a);
    let mut r_a: Vec<f64> = m.source.iter().zip(&y_a).map(|(b, a)| b - a).collect();
    let norm = m.norm_factor(x, &y_a);
    perf.initial_residual = sum_mag(&r_a) / norm;
    perf.final_residual = perf.initial_residual;

    if c.min_iter > 0 || !c.converged(perf.initial_residual, perf.final_residual) {
        let mut p_a = vec![0.0; n];
        let mut ay_a = vec![0.0; n];
        let mut s_a = vec![0.0; n];
        let mut z_a = vec![0.0; n];
        let mut t_a = vec![0.0; n];
        let r_a0 = r_a.clone();
        let (mut r_a0r_a, mut alpha, mut omega) = (0.0, 0.0, 0.0);
        let dilu = Dilu::new(m)?;
        loop {
            let r_a0r_a_old = r_a0r_a;
            r_a0r_a = dot(&r_a0, &r_a);
            if singular(r_a0r_a) {
                break;
            }
            if perf.iterations == 0 {
                p_a.copy_from_slice(&r_a);
            } else {
                if singular(omega) {
                    break;
                }
                let beta = (r_a0r_a / r_a0r_a_old) * (alpha / omega);
                for ((p, &r), &ay) in p_a.iter_mut().zip(&r_a).zip(&ay_a) {
                    *p = r + beta * (*p - omega * ay);
                }
            }
            dilu.precondition(m, &mut y_a, &p_a);
            m.amul(&y_a, &mut ay_a);
            alpha = r_a0r_a / dot(&r_a0, &ay_a);
            for ((s, &r), &ay) in s_a.iter_mut().zip(&r_a).zip(&ay_a) {
                *s = r - alpha * ay;
            }
            perf.final_residual = sum_mag(&s_a) / norm;
            if c.converged(perf.initial_residual, perf.final_residual) {
                for (xc, &y) in x.iter_mut().zip(&y_a) {
                    *xc += alpha * y;
                }
                perf.iterations += 1;
                break;
            }
            dilu.precondition(m, &mut z_a, &s_a);
            m.amul(&z_a, &mut t_a);
            omega = dot(&t_a, &s_a) / dot(&t_a, &t_a);
            for (((xc, r), (&y, &z)), (&s, &t)) in x
                .iter_mut()
                .zip(r_a.iter_mut())
                .zip(y_a.iter().zip(&z_a))
                .zip(s_a.iter().zip(&t_a))
            {
                *xc += alpha * y + omega * z;
                *r = s - omega * t;
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

/// One symmetric Gauss–Seidel sweep pair (forward, then backward) on
/// `A x = source`, as OpenFOAM's `symGaussSeidelSmoother` runs it: the faces
/// of a row are the ones it owns, and lower-triangle terms are pushed to the
/// rows below as each value is finished.
fn sym_gauss_seidel(m: &LduMatrix, x: &mut [f64], own_start: &[usize], b_prime: &mut [f64]) {
    let n = m.n_cells();
    let row = |c: usize| {
        let a = own_start.get(c).copied().unwrap_or(0);
        let b = own_start.get(c + 1).copied().unwrap_or(a);
        a..b
    };
    let relax = |c: usize, x: &mut [f64], b_prime: &mut [f64]| {
        let mut v = b_prime.get(c).copied().unwrap_or(0.0);
        for f in row(c) {
            let (Some(&up), Some(&u)) = (m.upper.get(f), m.u.get(f)) else {
                continue;
            };
            v -= up * x.get(u).copied().unwrap_or(0.0);
        }
        v /= m.diag.get(c).copied().unwrap_or(1.0);
        for f in row(c) {
            let (Some(&lo), Some(&u)) = (m.lower.get(f), m.u.get(f)) else {
                continue;
            };
            if let Some(b) = b_prime.get_mut(u) {
                *b -= lo * v;
            }
        }
        if let Some(xc) = x.get_mut(c) {
            *xc = v;
        }
    };
    b_prime.copy_from_slice(&m.source);
    for c in 0..n {
        relax(c, x, b_prime);
    }
    // The backward sweep starts from what the forward one left in `b_prime`,
    // as OpenFOAM's does.
    for c in (0..n).rev() {
        relax(c, x, b_prime);
    }
}

/// OpenFOAM's `smoothSolver` with the `symGaussSeidel` smoother: sweeps of
/// `n_sweeps` until converged.
pub fn smooth_sym_gauss_seidel(
    m: &LduMatrix,
    x: &mut [f64],
    c: &Controls,
    n_sweeps: usize,
) -> Result<Performance> {
    let n = m.n_cells();
    if x.len() != n || m.source.len() != n {
        return Err(FvError::Shape(format!("{} unknowns for {n} rows", x.len())));
    }
    if m.diag.iter().any(|&d| d == 0.0 || !d.is_finite()) {
        return Err(FvError::Singular("symGaussSeidel: a zero diagonal".into()));
    }
    let n_sweeps = n_sweeps.max(1);
    let mut perf = Performance {
        solver: "smoothSolver",
        initial_residual: 0.0,
        final_residual: 0.0,
        iterations: 0,
        converged: false,
    };
    let mut ax = vec![0.0; n];
    m.amul(x, &mut ax);
    let norm = m.norm_factor(x, &ax);
    perf.initial_residual = m
        .source
        .iter()
        .zip(&ax)
        .map(|(b, a)| (b - a).abs())
        .sum::<f64>()
        / norm;
    perf.final_residual = perf.initial_residual;

    if c.min_iter > 0 || !c.converged(perf.initial_residual, perf.final_residual) {
        // Where each row's owned faces start: faces are sorted by owner.
        let mut own_start = vec![0usize; n + 1];
        for &l in &m.l {
            if let Some(s) = own_start.get_mut(l + 1) {
                *s += 1;
            }
        }
        for i in 1..own_start.len() {
            let prev = own_start.get(i - 1).copied().unwrap_or(0);
            if let Some(s) = own_start.get_mut(i) {
                *s += prev;
            }
        }
        let mut b_prime = vec![0.0; n];
        loop {
            for _ in 0..n_sweeps {
                sym_gauss_seidel(m, x, &own_start, &mut b_prime);
            }
            perf.final_residual = sum_mag(&m.residual(x)) / norm;
            perf.iterations += n_sweeps;
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
    fn pbicgstab_solves_an_asymmetric_chain() {
        // Upwind convection plus diffusion along a chain: lower ≠ upper.
        let n = 40;
        let mut m = chain(n);
        m.lower = vec![-1.5; n - 1];
        m.upper = vec![-0.5; n - 1];
        let mut x = vec![0.0; n];
        let c = Controls {
            tolerance: 1e-14,
            ..Controls::default()
        };
        let perf = pbicgstab_dilu(&m, &mut x, &c).unwrap();
        assert!(perf.converged, "{perf}");
        let mut ax = vec![0.0; n];
        m.amul(&x, &mut ax);
        for (a, b) in ax.iter().zip(&m.source) {
            assert!((a - b).abs() < 1e-10, "{a} vs {b}");
        }
    }

    #[test]
    fn sym_gauss_seidel_solves_an_asymmetric_chain() {
        let mut m = chain(30);
        for (k, lo) in m.lower.iter_mut().enumerate() {
            *lo = -1.0 - 0.01 * k as f64;
        }
        for d in &mut m.diag {
            *d = 2.5;
        }
        let exact: Vec<f64> = (0..30).map(|i| (i as f64 * 0.3).sin()).collect();
        let mut b = vec![0.0; 30];
        m.amul(&exact, &mut b);
        m.source = b;
        let mut x = vec![0.0; 30];
        let c = Controls {
            tolerance: 1e-12,
            ..Controls::default()
        };
        let perf = smooth_sym_gauss_seidel(&m, &mut x, &c, 1).unwrap();
        assert!(perf.converged && perf.iterations > 1, "{perf}");
        for (a, e) in x.iter().zip(&exact) {
            assert!((a - e).abs() < 1e-10);
        }
    }

    #[test]
    fn bad_shapes_and_zero_pivots_are_errors() {
        let m = chain(3);
        assert!(pcg_dic(&m, &mut [0.0; 2], &Controls::default()).is_err());
        let mut z = chain(3);
        z.diag = vec![0.0; 3];
        assert!(pcg_dic(&z, &mut [0.0; 3], &Controls::default()).is_err());
        assert!(pbicgstab_dilu(&z, &mut [0.0; 3], &Controls::default()).is_err());
        assert!(pbicgstab_dilu(&m, &mut [0.0; 2], &Controls::default()).is_err());
    }
}
