//! `symGaussSeidel` on blocks of cells, one block per task.
//!
//! A Gauss–Seidel sweep is a chain: each row needs the one before it, so the
//! exact sweep OpenFOAM runs cannot be split over threads without changing it.
//! This is the parallel variant, and it is a different smoother: the cells are
//! cut into `n_blocks` contiguous ranges, each range does an exact symmetric
//! sweep of its own rows, and what couples one range to another is taken from
//! the values the sweep started with (block Jacobi over Gauss–Seidel blocks,
//! the way OpenFOAM's decomposed runs lag the processor interfaces).
//!
//! The ranges depend on `n_blocks` alone, never on how many threads run them,
//! so a solve gives the same bits on one thread and on sixteen. It does not
//! give the same iterations as the single-block smoother, which is why it is
//! asked for (`nBlocks` in the solver's dictionary) and never chosen.
//!
//! The residual the loop checks after each sweep is computed per cell from the
//! cell's own faces, in the order the face-by-face loop of
//! [`LduMatrix::residual`] visits them, so it is the same bit for bit and can
//! be split over the same blocks.

use std::sync::OnceLock;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::error::{FvError, Result};
use crate::ldu::LduMatrix;
use crate::solve::{Controls, Performance, smooth_sym_gauss_seidel, sum_mag};

/// Most blocks a solve is cut into.
pub const MAX_BLOCKS: usize = 1024;

/// How many threads a solve may use: `VORTIA_THREADS` when it is a positive
/// number, else what the machine offers.
pub fn thread_count() -> usize {
    static COUNT: OnceLock<usize> = OnceLock::new();
    *COUNT.get_or_init(|| {
        std::env::var("VORTIA_THREADS")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|&n| n > 0)
            .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
            .unwrap_or(1)
            .min(256)
    })
}

/// `smoothSolver` with `symGaussSeidel` on `n_blocks` blocks of cells.
///
/// With one block (or a matrix the blocks cannot be cut from) this is
/// [`smooth_sym_gauss_seidel`] itself.
pub fn smooth_sym_gauss_seidel_blocks(
    m: &LduMatrix,
    x: &mut [f64],
    c: &Controls,
    n_sweeps: usize,
    n_blocks: usize,
) -> Result<Performance> {
    smooth_blocks(m, x, c, n_sweeps, n_blocks, thread_count())
}

/// Addressing by row: which faces each cell owns, and which it is the
/// neighbour of.
struct Rows {
    own_start: Vec<usize>,
    nb_start: Vec<usize>,
    nb_faces: Vec<usize>,
}

impl Rows {
    /// `None` unless the faces are in upper-triangular order (owners
    /// ascending, owner below neighbour, everything in range), which the mesh
    /// guarantees and a hostile matrix may not.
    fn new(m: &LduMatrix) -> Option<Self> {
        let n = m.n_cells();
        let nf = m.l.len();
        if m.u.len() != nf || m.upper.len() != nf || m.lower.len() != nf || m.source.len() != n {
            return None;
        }
        let mut previous = 0;
        for (&l, &u) in m.l.iter().zip(&m.u) {
            if l < previous || l >= u || u >= n {
                return None;
            }
            previous = l;
        }
        let mut own_start = vec![0usize; n + 1];
        let mut nb_start = vec![0usize; n + 1];
        for (&l, &u) in m.l.iter().zip(&m.u) {
            *own_start.get_mut(l + 1)? += 1;
            *nb_start.get_mut(u + 1)? += 1;
        }
        for i in 1..=n {
            let (po, pn) = (*own_start.get(i - 1)?, *nb_start.get(i - 1)?);
            *own_start.get_mut(i)? += po;
            *nb_start.get_mut(i)? += pn;
        }
        // Faces by neighbour, each cell's in ascending face order.
        let mut fill = nb_start.clone();
        let mut nb_faces = vec![0usize; nf];
        for (f, &u) in m.u.iter().enumerate() {
            let slot = fill.get_mut(u)?;
            *nb_faces.get_mut(*slot)? = f;
            *slot += 1;
        }
        Some(Self {
            own_start,
            nb_start,
            nb_faces,
        })
    }

    fn owned(&self, c: usize) -> std::ops::Range<usize> {
        let a = self.own_start.get(c).copied().unwrap_or(0);
        a..self.own_start.get(c + 1).copied().unwrap_or(a)
    }

    fn neighbour_faces(&self, c: usize) -> &[usize] {
        let a = self.nb_start.get(c).copied().unwrap_or(0);
        let b = self.nb_start.get(c + 1).copied().unwrap_or(a);
        self.nb_faces.get(a..b).unwrap_or_default()
    }
}

/// A range of cells and what ties it to the others.
struct Block {
    start: usize,
    end: usize,
    /// `(cell, coefficient, other cell)` for each face that leaves the block,
    /// seen from the side inside it.
    cross: Vec<(usize, f64, usize)>,
    /// The right-hand side of the block's own sweeps.
    rhs: Vec<f64>,
}

struct Plan {
    rows: Rows,
    blocks: Vec<Block>,
    x_old: Vec<f64>,
    r: Vec<f64>,
    /// The threads the blocks run on; `None` runs them one after another.
    pool: Option<ThreadPool>,
}

impl Plan {
    fn new(m: &LduMatrix, n_blocks: usize, threads: usize) -> Option<Self> {
        let n = m.n_cells();
        let rows = Rows::new(m)?;
        let count = n_blocks.clamp(1, MAX_BLOCKS).min(n.max(1));
        let bounds: Vec<usize> = (0..=count).map(|k| n.saturating_mul(k) / count).collect();
        let mut blocks: Vec<Block> = bounds
            .windows(2)
            .map(|w| {
                let (start, end) = (
                    w.first().copied().unwrap_or(0),
                    w.get(1).copied().unwrap_or(0),
                );
                Block {
                    start,
                    end,
                    cross: Vec::new(),
                    rhs: vec![0.0; end.saturating_sub(start)],
                }
            })
            .collect();
        let block_of = |c: usize| bounds.partition_point(|&b| b <= c).saturating_sub(1);
        for (((&l, &u), &up), &lo) in m.l.iter().zip(&m.u).zip(&m.upper).zip(&m.lower) {
            let (bl, bu) = (block_of(l), block_of(u));
            if bl != bu {
                blocks.get_mut(bl)?.cross.push((l, up, u));
                blocks.get_mut(bu)?.cross.push((u, lo, l));
            }
        }
        Some(Self {
            rows,
            blocks,
            x_old: vec![0.0; n],
            r: vec![0.0; n],
            pool: (threads > 1)
                .then(|| ThreadPoolBuilder::new().num_threads(threads).build().ok())
                .flatten(),
        })
    }

    /// One symmetric sweep over every block, from the values in `x`.
    fn sweep(&mut self, m: &LduMatrix, x: &mut [f64]) {
        self.x_old.copy_from_slice(x);
        let (rows, x_old) = (&self.rows, &self.x_old);
        let mut rest: &mut [f64] = x;
        let mut work = Vec::with_capacity(self.blocks.len());
        for block in &mut self.blocks {
            let Some((head, tail)) =
                std::mem::take(&mut rest).split_at_mut_checked(block.rhs.len())
            else {
                return;
            };
            rest = tail;
            work.push((block, head));
        }
        for_each_parallel(self.pool.as_ref(), work, |(block, xs)| {
            sweep_block(m, rows, x_old, block, xs);
        });
    }

    /// `Σ |source − A x|`, the residual split over the blocks and summed in
    /// cell order.
    fn residual_sum(&mut self, m: &LduMatrix, x: &[f64]) -> f64 {
        let rows = &self.rows;
        let mut rest: &mut [f64] = &mut self.r;
        let mut work = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            let Some((head, tail)) =
                std::mem::take(&mut rest).split_at_mut_checked(block.rhs.len())
            else {
                return f64::NAN;
            };
            rest = tail;
            work.push((block.start, head));
        }
        for_each_parallel(self.pool.as_ref(), work, |(start, r)| {
            residual_rows(m, rows, x, start, r);
        });
        sum_mag(&self.r)
    }
}

/// Run `f` on every item, on the pool's threads when there is one.
fn for_each_parallel<T: Send>(
    pool: Option<&ThreadPool>,
    items: Vec<T>,
    f: impl Fn(T) + Sync + Send,
) {
    match pool {
        Some(pool) => pool.install(|| items.into_par_iter().for_each(f)),
        None => items.into_iter().for_each(f),
    }
}

/// The block's forward and backward sweep on its own cells (`x` is the
/// block's slice of the solution).
fn sweep_block(m: &LduMatrix, rows: &Rows, x_old: &[f64], block: &mut Block, x: &mut [f64]) {
    let (start, end) = (block.start, block.end);
    // What the other blocks contribute, from where they were.
    for (b, &s) in block
        .rhs
        .iter_mut()
        .zip(m.source.get(start..end).unwrap_or_default())
    {
        *b = s;
    }
    for &(cell, coefficient, other) in &block.cross {
        if let (Some(b), Some(&xo)) = (
            block.rhs.get_mut(cell.saturating_sub(start)),
            x_old.get(other),
        ) {
            *b -= coefficient * xo;
        }
    }
    let mut relax = |c: usize, rhs: &mut [f64], push: bool| {
        let local = c.saturating_sub(start);
        let faces = rows.owned(c);
        let (Some(upper), Some(lower), Some(neighbours)) = (
            m.upper.get(faces.clone()),
            m.lower.get(faces.clone()),
            m.u.get(faces),
        ) else {
            return;
        };
        let mut v = rhs.get(local).copied().unwrap_or(0.0);
        for (&up, &u) in upper.iter().zip(neighbours) {
            if u < end {
                v -= up * x.get(u.saturating_sub(start)).copied().unwrap_or(0.0);
            }
        }
        v /= m.diag.get(c).copied().unwrap_or(1.0);
        if push {
            for (&lo, &u) in lower.iter().zip(neighbours) {
                if u < end
                    && let Some(b) = rhs.get_mut(u.saturating_sub(start))
                {
                    *b -= lo * v;
                }
            }
        }
        if let Some(xc) = x.get_mut(local) {
            *xc = v;
        }
    };
    let mut rhs = std::mem::take(&mut block.rhs);
    for c in start..end {
        relax(c, &mut rhs, true);
    }
    // The backward sweep starts from what the forward one left in the
    // right-hand side. What it would push goes to rows it has already
    // finished, and the next sweep starts over, so it is left out.
    for c in (start..end).rev() {
        relax(c, &mut rhs, false);
    }
    block.rhs = rhs;
}

/// The residual of the rows `start..start + r.len()`, row by row.
fn residual_rows(m: &LduMatrix, rows: &Rows, x: &[f64], start: usize, r: &mut [f64]) {
    for (i, out) in r.iter_mut().enumerate() {
        let c = start + i;
        let mut v = m.source.get(c).copied().unwrap_or(0.0)
            - m.diag.get(c).copied().unwrap_or(0.0) * x.get(c).copied().unwrap_or(0.0);
        // The faces this row is the neighbour of come before the ones it
        // owns in face order, so this is the order the face loop adds in.
        for &f in rows.neighbour_faces(c) {
            let l = m.l.get(f).copied().unwrap_or(0);
            v -= m.lower.get(f).copied().unwrap_or(0.0) * x.get(l).copied().unwrap_or(0.0);
        }
        for f in rows.owned(c) {
            let u = m.u.get(f).copied().unwrap_or(0);
            v -= m.upper.get(f).copied().unwrap_or(0.0) * x.get(u).copied().unwrap_or(0.0);
        }
        *out = v;
    }
}

fn smooth_blocks(
    m: &LduMatrix,
    x: &mut [f64],
    c: &Controls,
    n_sweeps: usize,
    n_blocks: usize,
    threads: usize,
) -> Result<Performance> {
    let n = m.n_cells();
    let plan = if n_blocks > 1 && x.len() == n {
        Plan::new(m, n_blocks, threads)
    } else {
        None
    };
    let Some(mut plan) = plan else {
        return smooth_sym_gauss_seidel(m, x, c, n_sweeps);
    };
    if m.diag.iter().any(|&d| d == 0.0 || !d.is_finite()) {
        return Err(FvError::Singular("symGaussSeidel: a zero diagonal".into()));
    }
    let n_sweeps = n_sweeps.max(1);
    let mut ax = vec![0.0; n];
    m.amul(x, &mut ax);
    let norm = m.norm_factor(x, &ax);
    let initial = m
        .source
        .iter()
        .zip(&ax)
        .map(|(b, a)| (b - a).abs())
        .sum::<f64>()
        / norm;
    let mut perf = Performance {
        solver: "smoothSolver",
        initial_residual: initial,
        final_residual: initial,
        iterations: 0,
        converged: false,
    };
    if c.min_iter > 0 || !c.converged(perf.initial_residual, perf.final_residual) {
        loop {
            for _ in 0..n_sweeps {
                plan.sweep(m, x);
            }
            perf.final_residual = plan.residual_sum(m, x) / norm;
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

    /// A `w × h` grid of cells, faces in the order a mesh numbers them
    /// (owners ascending), with a convection-like asymmetry and a diagonal
    /// that dominates.
    fn grid(w: usize, h: usize) -> LduMatrix {
        let n = w * h;
        let (mut l, mut u) = (Vec::new(), Vec::new());
        for c in 0..n {
            let (i, j) = (c % w, c / w);
            if i + 1 < w {
                l.push(c);
                u.push(c + 1);
            }
            if j + 1 < h {
                l.push(c);
                u.push(c + w);
            }
        }
        let nf = l.len();
        let upper: Vec<f64> = (0..nf).map(|f| -1.0 - 0.1 * ((f * 7 % 5) as f64)).collect();
        let lower: Vec<f64> = (0..nf)
            .map(|f| -1.0 + 0.05 * ((f * 3 % 7) as f64))
            .collect();
        let mut diag = vec![0.5; n];
        for f in 0..nf {
            diag[l[f]] -= lower[f];
            diag[u[f]] -= upper[f];
        }
        let source: Vec<f64> = (0..n)
            .map(|c| 1.0 + ((c * 13 % 11) as f64) / 10.0)
            .collect();
        LduMatrix {
            diag,
            upper,
            lower,
            source,
            l,
            u,
        }
    }

    fn tight() -> Controls {
        Controls {
            tolerance: 1e-12,
            rel_tol: 0.0,
            max_iter: 5000,
            min_iter: 0,
        }
    }

    fn start(n: usize) -> Vec<f64> {
        (0..n).map(|c| ((c * 5 % 13) as f64) / 7.0).collect()
    }

    #[test]
    fn the_row_residual_is_the_face_loop_residual_bit_for_bit() {
        let m = grid(23, 17);
        let x = start(m.n_cells());
        let rows = Rows::new(&m).unwrap();
        let mut got = vec![0.0; m.n_cells()];
        residual_rows(&m, &rows, &x, 0, &mut got);
        let want = m.residual(&x);
        assert_eq!(
            got.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            want.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        // And a slice in the middle, as a block asks for it.
        let mut part = vec![0.0; 100];
        residual_rows(&m, &rows, &x, 150, &mut part);
        assert_eq!(
            part.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            want[150..250]
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_answer_does_not_depend_on_the_number_of_threads() {
        let m = grid(40, 30);
        let run = |threads| {
            let mut x = start(m.n_cells());
            let perf = smooth_blocks(&m, &mut x, &tight(), 1, 7, threads).unwrap();
            (x.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), perf)
        };
        let (x1, p1) = run(1);
        assert!(p1.converged, "{p1}");
        for threads in [2, 3, 7, 16] {
            let (xt, pt) = run(threads);
            assert_eq!(xt, x1, "{threads} threads");
            assert_eq!(pt, p1, "{threads} threads");
        }
    }

    #[test]
    fn blocks_converge_to_the_solution_the_single_sweep_finds() {
        let m = grid(40, 30);
        let mut serial = start(m.n_cells());
        let ps = smooth_sym_gauss_seidel(&m, &mut serial, &tight(), 1).unwrap();
        let mut blocked = start(m.n_cells());
        let pb = smooth_blocks(&m, &mut blocked, &tight(), 1, 6, 3).unwrap();
        assert!(ps.converged && pb.converged, "{ps} / {pb}");
        let worst = serial
            .iter()
            .zip(&blocked)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-9, "solutions differ by {worst}");
        assert_eq!(ps.initial_residual, pb.initial_residual);
    }

    #[test]
    fn one_block_is_the_single_sweep_exactly() {
        let m = grid(20, 20);
        let mut a = start(m.n_cells());
        let mut b = a.clone();
        let pa = smooth_sym_gauss_seidel(&m, &mut a, &Controls::default(), 2).unwrap();
        let pb = smooth_blocks(&m, &mut b, &Controls::default(), 2, 1, 4).unwrap();
        assert_eq!(a, b);
        assert_eq!(pa, pb);
    }

    #[test]
    fn more_blocks_than_cells_or_a_hostile_matrix_does_not_panic() {
        let m = grid(3, 2);
        let mut x = start(m.n_cells());
        let p = smooth_blocks(&m, &mut x, &tight(), 1, 5000, 8).unwrap();
        assert!(p.converged);

        // Faces out of order, a neighbour out of range, a face pointing back:
        // the blocks are not cut, and the single sweep answers (or errors).
        for bad in [
            LduMatrix {
                l: vec![1, 0],
                u: vec![2, 1],
                upper: vec![-1.0; 2],
                lower: vec![-1.0; 2],
                diag: vec![3.0; 3],
                source: vec![1.0; 3],
            },
            LduMatrix {
                l: vec![0, 1],
                u: vec![1, 9],
                upper: vec![-1.0; 2],
                lower: vec![-1.0; 2],
                diag: vec![3.0; 3],
                source: vec![1.0; 3],
            },
            LduMatrix {
                l: vec![1],
                u: vec![0],
                upper: vec![-1.0],
                lower: vec![-1.0],
                diag: vec![3.0; 3],
                source: vec![1.0; 3],
            },
        ] {
            let mut x = vec![0.0; 3];
            let _ = smooth_blocks(&bad, &mut x, &tight(), 1, 2, 2);
        }
        // A wrong-sized solution is the single sweep's error, not a panic.
        let mut short = vec![0.0; 2];
        assert!(smooth_blocks(&m, &mut short, &tight(), 1, 2, 2).is_err());
    }

    #[test]
    fn a_zero_diagonal_is_an_error() {
        let mut m = grid(6, 6);
        m.diag[10] = 0.0;
        let mut x = start(m.n_cells());
        assert!(smooth_blocks(&m, &mut x, &tight(), 1, 3, 2).is_err());
    }
}
