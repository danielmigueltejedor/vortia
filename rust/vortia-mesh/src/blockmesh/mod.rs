//! The engine's own `blockMesh`: a `blockMeshDict` of hexahedral blocks with
//! grading, merged along their shared faces, as the five files of a
//! `polyMesh`.
//!
//! Derived from OpenFOAM's `blockMesh` (GPL-3.0-or-later, like Vortia), so
//! that the same dictionary gives the same mesh, point for point and face for
//! face: the transfinite interpolation of each block's points from its twelve
//! edges, the merge of the points along shared faces (by distance, or with
//! `fastMerge` by the blocks' topology), and the order of cells,
//! faces and patches.
//!
//! Supported: `line`, `arc`, `polyLine`, `spline`, `BSpline` and `bezier`
//! edges, `simpleGrading` and `edgeGrading` (with several sections),
//! `boundary` patches, `defaultPatch`, `scale`, vertices that carry a name
//! (and `namedVertices`, `namedBlocks`), and the cell zones that blocks name.
//! The rest (faces projected on geometry, projected edges, `mergePatchPairs`)
//! is refused with a message that says which.

mod edges;
mod names;
mod shapes;

use vortia_core::Vec3;
use vortia_foamfile::{Dictionary, Entry, FaceList, RawCellZone, RawPatch, RawPolyMesh, Value};

use crate::error::{MeshError, Result};
use edges::CurvedEdge;
use names::Names;
use shapes::{Cells, HEX_FACES, Shape, collapse_hex};

/// The most cells a dictionary may ask for in all.
pub const MAX_CELLS: usize = 1 << 27;

/// The most blocks a dictionary may hold.
const MAX_BLOCKS: usize = 1 << 16;

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

/// One section of an edge's grading: the share of the edge's length, the
/// share of its divisions, and how much longer the last cell is than the first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Section {
    block_fraction: f64,
    div_fraction: f64,
    expansion: f64,
}

impl Section {
    /// `gradingDescriptor(expansionRatio)`: all the edge, a negative ratio
    /// meaning its reciprocal.
    fn uniform(expansion: f64) -> Self {
        Self {
            block_fraction: 1.0,
            div_fraction: 1.0,
            expansion: if expansion < 0.0 {
                1.0 / (-expansion)
            } else {
                expansion
            },
        }
    }
}

/// `gradingDescriptors::inv`: the sections of the same edge walked the
/// other way round.
fn inverse(sections: &[Section]) -> Vec<Section> {
    sections
        .iter()
        .rev()
        .map(|s| Section {
            block_fraction: s.block_fraction,
            div_fraction: s.div_fraction,
            expansion: 1.0 / s.expansion,
        })
        .collect()
}

/// The vertices (by place in the block) each of a block's twelve edges runs
/// between: four along x, four along y, four along z.
const EDGE_ENDS: [(usize, usize); 12] = [
    (0, 1),
    (3, 2),
    (7, 6),
    (4, 5),
    (0, 3),
    (1, 2),
    (5, 6),
    (4, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// The fractions along an edge where its `n_div` divisions fall
/// (`lineDivide`).
fn divisions(n_div: usize, sections: &[Section]) -> Vec<f64> {
    let mut d = vec![0.0; n_div + 1];
    if let Some(last) = d.last_mut() {
        *last = 1.0;
    }
    if n_div < sections.len() || sections.is_empty() {
        // Fewer divisions than sections: uniform.
        for i in 1..n_div {
            if let Some(slot) = d.get_mut(i) {
                *slot = i as f64 / n_div as f64;
            }
        }
        return d;
    }
    let mut counts = Vec::with_capacity(sections.len());
    let mut sum = 0usize;
    let mut largest = 0usize;
    for (i, s) in sections.iter().enumerate() {
        let n = (s.div_fraction * n_div as f64 + 0.5) as usize;
        counts.push(n);
        sum += n;
        if sections
            .get(largest)
            .is_none_or(|l| s.div_fraction > l.div_fraction)
        {
            largest = i;
        }
    }
    if sum != n_div
        && let Some(c) = counts.get_mut(largest)
    {
        *c = (*c + n_div).saturating_sub(sum);
    }
    let mut start = 0.0;
    let mut first = 1usize;
    for (s, &count) in sections.iter().zip(&counts) {
        let end = first + count;
        if (s.expansion - 1.0).abs() <= 1e-300 {
            for i in first..end {
                if let Some(slot) = d.get_mut(i) {
                    *slot = start + s.block_fraction * ((i - first + 1) as f64) / count as f64;
                }
            }
        } else {
            let factor = if count > 1 {
                s.expansion.powf(1.0 / (count as f64 - 1.0))
            } else {
                0.0
            };
            for i in first..end {
                if let Some(slot) = d.get_mut(i) {
                    *slot = start
                        + s.block_fraction * (1.0 - factor.powf((i - first + 1) as f64))
                            / (1.0 - factor.powf(count as f64));
                }
            }
        }
        start = d.get(end.wrapping_sub(1)).copied().unwrap_or(start);
        first = end;
    }
    d
}

/// A block: its eight vertices, its divisions and the grading of its twelve
/// edges, and the points those make.
#[derive(Debug)]
struct Block {
    vertices: [usize; 8],
    n: [usize; 3],
    expansion: [Vec<Section>; 12],
    points: Vec<Vec3>,
    /// The cell zone the block's cells belong to, if it names one.
    zone: Option<String>,
}

impl Block {
    fn point(&self, i: usize, j: usize, k: usize) -> usize {
        i + (self.n[0] + 1) * (j + (self.n[1] + 1) * k)
    }

    fn n_points(&self) -> usize {
        (self.n[0] + 1) * (self.n[1] + 1) * (self.n[2] + 1)
    }

    fn n_cells(&self) -> usize {
        self.n[0] * self.n[1] * self.n[2]
    }

    /// The points and fractions of edge `e` (`blockDescriptor::edgePointsWeights`):
    /// where a listed curve runs between its two vertices the points are the
    /// curve's, otherwise there are none (the edge is straight).
    fn edge(&self, e: usize, curves: &[CurvedEdge]) -> (Option<Vec<Vec3>>, Vec<f64>) {
        let n = match e {
            0..=3 => self.n[0],
            4..=7 => self.n[1],
            _ => self.n[2],
        };
        let sections = self.expansion.get(e).map_or(&[][..], |s| &s[..]);
        let label = |local: usize| self.vertices.get(local).copied().unwrap_or(usize::MAX);
        let Some(&(a, b)) = EDGE_ENDS.get(e) else {
            return (None, divisions(n, sections));
        };
        for curve in curves {
            match curve.compare(label(a), label(b)) {
                0 => {}
                1 => {
                    let d = divisions(n, sections);
                    let p = d.iter().map(|&l| curve.position(l)).collect();
                    return (Some(p), d);
                }
                _ => {
                    let d = divisions(n, &inverse(sections));
                    let p: Vec<Vec3> = d.iter().rev().map(|&l| curve.position(l)).collect();
                    let w = d.iter().rev().map(|&l| 1.0 - l).collect();
                    return (Some(p), w);
                }
            }
        }
        (None, divisions(n, sections))
    }

    /// `block::createPoints`: the points of the twelve edges, and each point
    /// of the block a blend of them, corrected towards the curved edges.
    fn create_points(&mut self, corners: &[Vec3; 8], curves: &[CurvedEdge]) -> Result<()> {
        let [ni, nj, nk] = self.n;
        let (curved, w): (Vec<Option<Vec<Vec3>>>, Vec<Vec<f64>>) =
            (0..12).map(|e| self.edge(e, curves)).unzip();
        let any_curved = curved.iter().any(Option::is_some);
        let c = |i: usize| corners.get(i).copied().unwrap_or_default();
        let (p000, p100, p110, p010) = (c(0), c(1), c(2), c(3));
        let (p001, p101, p111, p011) = (c(4), c(5), c(6), c(7));
        let mut points = vec![Vec3::default(); self.n_points()];
        let at = |e: usize, i: usize| w.get(e).and_then(|v| v.get(i)).copied().unwrap_or(0.0);
        for k in 0..=nk {
            for j in 0..=nj {
                for i in 0..=ni {
                    let label = self.point(i, j, k);
                    let on = |v: usize, n: usize| v == 0 || v == n;
                    let value = if on(i, ni) && on(j, nj) && on(k, nk) {
                        match (i == 0, j == 0, k == 0) {
                            (true, true, true) => p000,
                            (false, true, true) => p100,
                            (false, false, true) => p110,
                            (true, false, true) => p010,
                            (true, true, false) => p001,
                            (false, true, false) => p101,
                            (false, false, false) => p111,
                            (true, false, false) => p011,
                        }
                    } else {
                        let (w0, w1, w2, w3) = (at(0, i), at(1, i), at(2, i), at(3, i));
                        let (w4, w5, w6, w7) = (at(4, j), at(5, j), at(6, j), at(7, j));
                        let (w8, w9, w10, w11) = (at(8, k), at(9, k), at(10, k), at(11, k));

                        let mut wx1 =
                            (1.0 - w0) * (1.0 - w4) * (1.0 - w8) + w0 * (1.0 - w5) * (1.0 - w9);
                        let mut wx2 = (1.0 - w1) * w4 * (1.0 - w11) + w1 * w5 * (1.0 - w10);
                        let mut wx3 = (1.0 - w2) * w7 * w11 + w2 * w6 * w10;
                        let mut wx4 = (1.0 - w3) * (1.0 - w7) * w8 + w3 * (1.0 - w6) * w9;
                        let sum_wx = wx1 + wx2 + wx3 + wx4;
                        wx1 /= sum_wx;
                        wx2 /= sum_wx;
                        wx3 /= sum_wx;
                        wx4 /= sum_wx;

                        let mut wy1 =
                            (1.0 - w4) * (1.0 - w0) * (1.0 - w8) + w4 * (1.0 - w1) * (1.0 - w11);
                        let mut wy2 = (1.0 - w5) * w0 * (1.0 - w9) + w5 * w1 * (1.0 - w10);
                        let mut wy3 = (1.0 - w6) * w3 * w9 + w6 * w2 * w10;
                        let mut wy4 = (1.0 - w7) * (1.0 - w3) * w8 + w7 * (1.0 - w2) * w11;
                        let sum_wy = wy1 + wy2 + wy3 + wy4;
                        wy1 /= sum_wy;
                        wy2 /= sum_wy;
                        wy3 /= sum_wy;
                        wy4 /= sum_wy;

                        let mut wz1 =
                            (1.0 - w8) * (1.0 - w0) * (1.0 - w4) + w8 * (1.0 - w3) * (1.0 - w7);
                        let mut wz2 = (1.0 - w9) * w0 * (1.0 - w5) + w9 * w3 * (1.0 - w6);
                        let mut wz3 = (1.0 - w10) * w1 * w5 + w10 * w2 * w6;
                        let mut wz4 = (1.0 - w11) * (1.0 - w1) * w4 + w11 * (1.0 - w2) * w7;
                        let sum_wz = wz1 + wz2 + wz3 + wz4;
                        wz1 /= sum_wz;
                        wz2 /= sum_wz;
                        wz3 /= sum_wz;
                        wz4 /= sum_wz;

                        let edgex1 = p000 + (p100 - p000) * w0;
                        let edgex2 = p010 + (p110 - p010) * w1;
                        let edgex3 = p011 + (p111 - p011) * w2;
                        let edgex4 = p001 + (p101 - p001) * w3;

                        let edgey1 = p000 + (p010 - p000) * w4;
                        let edgey2 = p100 + (p110 - p100) * w5;
                        let edgey3 = p101 + (p111 - p101) * w6;
                        let edgey4 = p001 + (p011 - p001) * w7;

                        let edgez1 = p000 + (p001 - p000) * w8;
                        let edgez2 = p100 + (p101 - p100) * w9;
                        let edgez3 = p110 + (p111 - p110) * w10;
                        let edgez4 = p010 + (p011 - p010) * w11;

                        let straight = (edgex1 * wx1
                            + edgex2 * wx2
                            + edgex3 * wx3
                            + edgex4 * wx4
                            + edgey1 * wy1
                            + edgey2 * wy2
                            + edgey3 * wy3
                            + edgey4 * wy4
                            + edgez1 * wz1
                            + edgez2 * wz2
                            + edgez3 * wz3
                            + edgez4 * wz4)
                            / 3.0;
                        if any_curved {
                            let at = |e: usize, i: usize| {
                                curved
                                    .get(e)
                                    .and_then(Option::as_ref)
                                    .and_then(|p| p.get(i))
                                    .copied()
                            };
                            let terms = [
                                (at(0, i), edgex1, wx1),
                                (at(1, i), edgex2, wx2),
                                (at(2, i), edgex3, wx3),
                                (at(3, i), edgex4, wx4),
                                (at(4, j), edgey1, wy1),
                                (at(5, j), edgey2, wy2),
                                (at(6, j), edgey3, wy3),
                                (at(7, j), edgey4, wy4),
                                (at(8, k), edgez1, wz1),
                                (at(9, k), edgez2, wz2),
                                (at(10, k), edgez3, wz3),
                                (at(11, k), edgez4, wz4),
                            ];
                            let mut correction = Vec3::default();
                            for (on_curve, on_line, weight) in terms {
                                if let Some(p) = on_curve {
                                    correction = correction + (p - on_line) * weight;
                                }
                            }
                            straight + correction
                        } else {
                            straight
                        }
                    };
                    if let Some(slot) = points.get_mut(label) {
                        *slot = value;
                    }
                }
            }
        }
        if points
            .iter()
            .any(|p| !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()))
        {
            return invalid("a block has points that are not numbers; check its edges");
        }
        self.points = points;
        Ok(())
    }

    /// The quadrilaterals on one of the block's six sides, as the block's
    /// own point labels (`block::createBoundary`).
    fn side(&self, side: usize) -> Vec<[usize; 4]> {
        let [ni, nj, nk] = self.n;
        let mut out = Vec::new();
        match side {
            0 | 1 => {
                let i = if side == 0 { 0 } else { ni };
                for k in 0..nk {
                    for j in 0..nj {
                        out.push(if side == 0 {
                            [
                                self.point(i, j, k),
                                self.point(i, j, k + 1),
                                self.point(i, j + 1, k + 1),
                                self.point(i, j + 1, k),
                            ]
                        } else {
                            [
                                self.point(i, j, k),
                                self.point(i, j + 1, k),
                                self.point(i, j + 1, k + 1),
                                self.point(i, j, k + 1),
                            ]
                        });
                    }
                }
            }
            2 | 3 => {
                let j = if side == 2 { 0 } else { nj };
                for i in 0..ni {
                    for k in 0..nk {
                        out.push(if side == 2 {
                            [
                                self.point(i, j, k),
                                self.point(i + 1, j, k),
                                self.point(i + 1, j, k + 1),
                                self.point(i, j, k + 1),
                            ]
                        } else {
                            [
                                self.point(i, j, k),
                                self.point(i, j, k + 1),
                                self.point(i + 1, j, k + 1),
                                self.point(i + 1, j, k),
                            ]
                        });
                    }
                }
            }
            _ => {
                let k = if side == 4 { 0 } else { nk };
                for i in 0..ni {
                    for j in 0..nj {
                        out.push(if side == 4 {
                            [
                                self.point(i, j, k),
                                self.point(i, j + 1, k),
                                self.point(i + 1, j + 1, k),
                                self.point(i + 1, j, k),
                            ]
                        } else {
                            [
                                self.point(i, j, k),
                                self.point(i + 1, j, k),
                                self.point(i + 1, j + 1, k),
                                self.point(i, j + 1, k),
                            ]
                        });
                    }
                }
            }
        }
        out
    }

    /// The eight point labels of cell `(i, j, k)`.
    fn cell(&self, i: usize, j: usize, k: usize) -> [usize; 8] {
        [
            self.point(i, j, k),
            self.point(i + 1, j, k),
            self.point(i + 1, j + 1, k),
            self.point(i, j + 1, k),
            self.point(i, j, k + 1),
            self.point(i + 1, j, k + 1),
            self.point(i + 1, j + 1, k + 1),
            self.point(i, j + 1, k + 1),
        ]
    }
}

fn number(v: &Value, what: &str) -> Result<f64> {
    match v.as_scalar() {
        Some(x) if x.is_finite() => Ok(x),
        _ => invalid(format!("{what}: expected a number")),
    }
}

fn count(v: &Value, what: &str) -> Result<usize> {
    v.as_label()
        .and_then(|n| usize::try_from(n).ok())
        .map_or_else(|| invalid(format!("{what}: expected a whole number")), Ok)
}

fn list<'a>(v: &'a Value, what: &str) -> Result<&'a [Value]> {
    v.as_list()
        .map_or_else(|| invalid(format!("{what}: expected a list")), Ok)
}

/// The vertices, and the names the list gives them: an entry is `(x y z)` or
/// `name <name> (x y z)`.
fn vertices(dict: &Dictionary, names: &mut Names) -> Result<Vec<Vec3>> {
    let items = dict
        .value("vertices")
        .and_then(Value::as_list)
        .ok_or_else(|| MeshError::Invalid("blockMeshDict has no `vertices`".into()))?;
    let mut out = Vec::new();
    let mut it = items.iter();
    while let Some(item) = it.next() {
        let i = out.len();
        let point = match item {
            Value::List(_) => item,
            Value::Word(kind) if kind == "name" => {
                let name = it
                    .next()
                    .and_then(Value::as_word)
                    .ok_or_else(|| MeshError::Invalid(format!("vertex {i}: no name")))?;
                names.name_vertex(name, i);
                it.next()
                    .filter(|v| matches!(v, Value::List(_)))
                    .ok_or_else(|| MeshError::Invalid(format!("vertex {i}: expected (x y z)")))?
            }
            Value::Word(_) | Value::Str(_) | Value::Keyed(..) => {
                return invalid("vertices projected onto geometry are not supported yet");
            }
            _ => return invalid(format!("vertex {i}: expected (x y z)")),
        };
        let xyz = list(point, "vertex")?;
        let [x, y, z] = xyz else {
            return invalid(format!("vertex {i}: expected (x y z)"));
        };
        out.push(Vec3::new(
            number(x, "vertex")?,
            number(y, "vertex")?,
            number(z, "vertex")?,
        ));
    }
    Ok(out)
}

/// One grading entry: a number, or a list of `(block fraction, division
/// fraction, expansion)` sections.
fn grading(v: &Value) -> Result<Vec<Section>> {
    if v.as_scalar().is_some() {
        return Ok(vec![Section::uniform(number(v, "grading")?)]);
    }
    let sections = list(v, "grading")?;
    let mut out = Vec::with_capacity(sections.len());
    for s in sections {
        let parts = list(s, "grading section")?;
        let [block, div, expansion] = parts else {
            return invalid("a grading section is (block fraction, division fraction, expansion)");
        };
        out.push(Section {
            block_fraction: number(block, "grading section")?,
            div_fraction: number(div, "grading section")?,
            expansion: number(expansion, "grading section")?,
        });
    }
    let (sum_block, sum_div) = out.iter().fold((0.0, 0.0), |(a, b), s| {
        (a + s.block_fraction, b + s.div_fraction)
    });
    if out.is_empty() || !(sum_block > 0.0 && sum_div > 0.0) {
        return invalid("a grading needs sections with positive fractions");
    }
    for s in &mut out {
        s.block_fraction /= sum_block;
        s.div_fraction /= sum_div;
    }
    Ok(out)
}

fn blocks(dict: &Dictionary, n_vertices: usize, names: &Names) -> Result<Vec<Block>> {
    let items = dict
        .value("blocks")
        .and_then(Value::as_list)
        .ok_or_else(|| MeshError::Invalid("blockMeshDict has no `blocks`".into()))?;
    let mut out = Vec::new();
    let mut it = items.iter().peekable();
    while let Some(kind) = it.next() {
        if kind.as_word() != Some("hex") {
            return invalid(format!(
                "block {}: only `hex` blocks are supported, found {kind:?}",
                out.len()
            ));
        }
        let label = format!("block {}", out.len());
        let verts = it
            .next()
            .ok_or_else(|| MeshError::Invalid(format!("{label}: no vertices")))?;
        let verts = list(verts, &label)?;
        if verts.len() != 8 {
            return invalid(format!("{label}: a hex has eight vertices"));
        }
        let mut vertices = [0usize; 8];
        for (slot, v) in vertices.iter_mut().zip(verts) {
            let n = names.vertex(v, &label)?;
            if n >= n_vertices {
                return invalid(format!("{label}: vertex {n} does not exist"));
            }
            *slot = n;
        }
        let zone = match it.peek().and_then(|v| v.as_word()) {
            Some(zone) => {
                let zone = zone.to_owned();
                it.next();
                Some(zone)
            }
            None => None,
        };
        let cells = it
            .next()
            .ok_or_else(|| MeshError::Invalid(format!("{label}: no divisions")))?;
        let cells = list(cells, &label)?;
        if cells.len() != 3 {
            return invalid(format!("{label}: divisions are (nx ny nz)"));
        }
        let mut n = [0usize; 3];
        for (slot, v) in n.iter_mut().zip(cells) {
            *slot = count(v, &label)?;
            if *slot == 0 {
                return invalid(format!(
                    "{label}: a block needs at least one cell per direction"
                ));
            }
        }
        n[0].checked_mul(n[1])
            .and_then(|v| v.checked_mul(n[2]))
            .filter(|&c| c <= MAX_CELLS)
            .ok_or_else(|| MeshError::Invalid(format!("{label}: too many cells")))?;
        let mut expansion: [Vec<Section>; 12] =
            std::array::from_fn(|_| vec![Section::uniform(1.0)]);
        if let Some(word) = it.peek().and_then(|v| v.as_word()) {
            let word = word.to_owned();
            if word != "hex" {
                it.next();
                let g = it
                    .next()
                    .ok_or_else(|| MeshError::Invalid(format!("{label}: no grading")))?;
                let g = list(g, &label)?;
                match (word.as_str(), g.len()) {
                    ("simpleGrading", 3) => {
                        for (axis, v) in g.iter().enumerate() {
                            let s = grading(v)?;
                            for e in 0..4 {
                                if let Some(slot) = expansion.get_mut(axis * 4 + e) {
                                    *slot = s.clone();
                                }
                            }
                        }
                    }
                    ("edgeGrading", 12) => {
                        for (slot, v) in expansion.iter_mut().zip(g) {
                            *slot = grading(v)?;
                        }
                    }
                    ("simpleGrading", _) => {
                        return invalid(format!("{label}: simpleGrading takes three entries"));
                    }
                    ("edgeGrading", _) => {
                        return invalid(format!("{label}: edgeGrading takes twelve entries"));
                    }
                    _ => return invalid(format!("{label}: unknown grading `{word}`")),
                }
            }
        }
        out.push(Block {
            vertices,
            n,
            expansion,
            points: Vec::new(),
            zone,
        });
        if out.len() > MAX_BLOCKS {
            return invalid("too many blocks");
        }
    }
    if out.is_empty() {
        return invalid("`blocks` holds no block");
    }
    let all: usize = out
        .iter()
        .map(Block::n_cells)
        .fold(0, usize::saturating_add);
    if all > MAX_CELLS {
        return invalid(format!("{all} cells is more than {MAX_CELLS}"));
    }
    Ok(out)
}

/// A boundary patch of the dictionary: its name, its entry and the block
/// faces it holds as vertex labels.
struct PatchDef {
    name: String,
    dict: Dictionary,
    faces: Vec<Vec<usize>>,
}

fn boundary(dict: &Dictionary, n_vertices: usize, names: &Names) -> Result<Vec<PatchDef>> {
    if dict.get("patches").is_some() && dict.get("boundary").is_none() {
        return invalid("the old `patches` section is not supported; use `boundary`");
    }
    let Some(items) = dict.value("boundary").and_then(Value::as_list) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<PatchDef> = Vec::new();
    for item in items {
        let Value::Keyed(name, entry) = item else {
            return invalid("`boundary` holds patches written `name { ... }`");
        };
        if out.iter().any(|p| &p.name == name) {
            return invalid(format!("patch {name} appears twice"));
        }
        let mut faces = Vec::new();
        for f in entry
            .value("faces")
            .and_then(Value::as_list)
            .unwrap_or_default()
        {
            let f = list(f, name)?;
            if f.len() == 2 {
                return invalid(format!(
                    "patch {name}: faces written (block face) are not supported yet"
                ));
            }
            let mut face = Vec::with_capacity(f.len());
            for v in f {
                let n = names.face_label(v, name)?;
                if n >= n_vertices {
                    return invalid(format!("patch {name}: vertex {n} does not exist"));
                }
                face.push(n);
            }
            faces.push(face);
        }
        out.push(PatchDef {
            name: name.clone(),
            dict: entry.clone(),
            faces,
        });
    }
    Ok(out)
}

/// The blocks' own mesh: which block faces meet, as the topology the
/// merge goes by.
struct Topology {
    built: shapes::Built,
    n_blocks: usize,
}

impl Topology {
    fn new(blocks: &[Block], patches: &[PatchDef]) -> Result<Self> {
        let mut cells = Cells::new();
        for b in blocks {
            let faces: Vec<Vec<usize>> = HEX_FACES
                .iter()
                .map(|f| {
                    f.iter()
                        .filter_map(|&i| b.vertices.get(i).copied())
                        .collect()
                })
                .collect();
            cells.push(&faces);
        }
        let patch_faces: Vec<Vec<Vec<usize>>> = patches.iter().map(|p| p.faces.clone()).collect();
        let built = shapes::build(&cells, &patch_faces)?;
        Ok(Self {
            built,
            n_blocks: blocks.len(),
        })
    }

    /// The side of `block` that a topology face is.
    fn side_of(&self, block: usize, face: usize) -> Option<usize> {
        let start = block * 6;
        (0..6).find(|&side| self.built.cell_face.get(start + side) == Some(&face))
    }

    /// The block and the side of it that a topology face is.
    fn locate(&self, face: usize) -> Option<(usize, usize)> {
        (0..self.n_blocks).find_map(|block| self.side_of(block, face).map(|s| (block, s)))
    }
}

/// The corners of a block's side as hex vertices, in the side's own grid
/// order: `(0, 0)`, `(last, 0)`, `(0, last)`, `(last, last)`.
const SIDE_CORNERS: [[usize; 4]; 6] = [
    [0, 3, 4, 7],
    [1, 2, 5, 6],
    [0, 1, 4, 5],
    [3, 2, 7, 6],
    [0, 1, 3, 2],
    [4, 5, 7, 6],
];

/// The number of points of a side's grid in each direction.
fn side_grid(b: &Block, side: usize) -> (usize, usize) {
    match side / 2 {
        0 => (b.n[1] + 1, b.n[2] + 1),
        1 => (b.n[0] + 1, b.n[2] + 1),
        _ => (b.n[0] + 1, b.n[1] + 1),
    }
}

/// The block's point on a side's grid.
fn side_point(b: &Block, side: usize, i: usize, j: usize) -> usize {
    match side {
        0 => b.point(0, i, j),
        1 => b.point(b.n[0], i, j),
        2 => b.point(i, 0, j),
        3 => b.point(i, b.n[1], j),
        4 => b.point(i, j, 0),
        _ => b.point(i, j, b.n[2]),
    }
}

/// How the grid of one side lies on the grid of the face it is glued to:
/// whether the axes swap, and whether each of the other's is reversed.
#[derive(Debug, Clone, Copy)]
struct Glue {
    swap: bool,
    flip_a: bool,
    flip_b: bool,
}

impl Glue {
    /// The point `(i, j)` of a side whose grid has `(pi, pj)` points, on the
    /// grid of the side it is glued to.
    fn map(self, (pi, pj): (usize, usize), i: usize, j: usize) -> (usize, usize) {
        let (a, b, na, nb) = if self.swap {
            (j, i, pj, pi)
        } else {
            (i, j, pi, pj)
        };
        (
            if self.flip_a { na - 1 - a } else { a },
            if self.flip_b { nb - 1 - b } else { b },
        )
    }
}

/// The way the side `p_side` of block `p` is glued to the side `n_side` of
/// block `n`, found by matching the vertices at the four corners.
fn glue(p: &Block, p_side: usize, n: &Block, n_side: usize) -> Result<Glue> {
    let p_grid = side_grid(p, p_side);
    let n_grid = side_grid(n, n_side);
    let corner = |b: &Block, side: usize, c: usize| -> Option<usize> {
        SIDE_CORNERS
            .get(side)
            .and_then(|s| s.get(c))
            .and_then(|&v| b.vertices.get(v))
            .copied()
    };
    let ends = [
        (0, 0),
        (p_grid.0 - 1, 0),
        (0, p_grid.1 - 1),
        (p_grid.0 - 1, p_grid.1 - 1),
    ];
    for swap in [false, true] {
        let dims = if swap { (p_grid.1, p_grid.0) } else { p_grid };
        if dims != n_grid {
            continue;
        }
        for flip_a in [false, true] {
            for flip_b in [false, true] {
                let g = Glue {
                    swap,
                    flip_a,
                    flip_b,
                };
                let ok = ends.iter().enumerate().all(|(c, &(i, j))| {
                    let (a, b) = g.map(p_grid, i, j);
                    let nc = match (a == 0, b == 0) {
                        (true, true) => 0,
                        (false, true) => 1,
                        (true, false) => 2,
                        (false, false) => 3,
                    };
                    corner(p, p_side, c) == corner(n, n_side, nc)
                });
                if ok {
                    return Ok(g);
                }
            }
        }
    }
    invalid(
        "two blocks that share a face have vertices that do not correspond, \
         or divisions that differ along it",
    )
}

/// `mergeList` after the merge, and the number of points left.
struct Merge {
    list: Vec<usize>,
    n_points: usize,
}

fn offsets(blocks: &[Block]) -> Vec<usize> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut n = 0usize;
    for b in blocks {
        out.push(n);
        n += b.n_points();
    }
    out
}

fn renumber(list: &mut [i64]) -> Result<usize> {
    let mut next = 0i64;
    for p in 0..list.len() {
        let m = list.get(p).copied().unwrap_or(-1);
        if m > p as i64 {
            return invalid("the merge list holds a point out of range");
        }
        if m == -1 || m == p as i64 {
            if let Some(slot) = list.get_mut(p) {
                *slot = next;
            }
            next += 1;
        } else {
            let to = list.get(m as usize).copied().unwrap_or(-1);
            if let Some(slot) = list.get_mut(p) {
                *slot = to;
            }
        }
    }
    Ok(next as usize)
}

/// `blockMesh::calcTopologicalMerge`: along every internal face of the
/// topology, the points of the two grids that lie on each other.
fn merge_topology(blocks: &[Block], topology: &Topology, corners: &[[Vec3; 8]]) -> Result<Merge> {
    let offs = offsets(blocks);
    let total = blocks.iter().map(Block::n_points).sum::<usize>();
    let mut list = vec![-1i64; total];
    let built = &topology.built;
    for face in 0..built.n_internal {
        let bp = built.owner.get(face).copied().unwrap_or(0);
        let bn = built.neighbour.get(face).copied().unwrap_or(0);
        let (Some(sp), Some(sn)) = (topology.side_of(bp, face), topology.side_of(bn, face)) else {
            return invalid("a block face is not found in its blocks");
        };
        let (Some(p), Some(n)) = (blocks.get(bp), blocks.get(bn)) else {
            return invalid("a block face is not found in its blocks");
        };
        let g = glue(p, sp, n, sn)?;
        let (pi, pj) = side_grid(p, sp);
        let (op, on) = (
            offs.get(bp).copied().unwrap_or(0),
            offs.get(bn).copied().unwrap_or(0),
        );
        // OpenFOAM's sanity check: the glued points lie on each other to
        // within a millionth of the block's size.
        let (mut lo, mut hi) = (
            Vec3::new(f64::MAX, f64::MAX, f64::MAX),
            Vec3::new(f64::MIN, f64::MIN, f64::MIN),
        );
        for c in corners.get(bp).into_iter().flatten() {
            lo = Vec3::new(lo.x.min(c.x), lo.y.min(c.y), lo.z.min(c.z));
            hi = Vec3::new(hi.x.max(c.x), hi.y.max(c.y), hi.z.max(c.z));
        }
        let span = (hi - lo) * 1e-6;
        let tolerance = span.x * span.x + span.y * span.y + span.z * span.z;
        for j in 0..pj {
            for i in 0..pi {
                let a = side_point(p, sp, i, j);
                let (ni, nj) = g.map((pi, pj), i, j);
                let b = side_point(n, sn, ni, nj);
                let d = match (p.points.get(a), n.points.get(b)) {
                    (Some(&x), Some(&y)) => {
                        let d = x - y;
                        d.x * d.x + d.y * d.y + d.z * d.z
                    }
                    _ => f64::MAX,
                };
                if d > tolerance {
                    return invalid(format!(
                        "points of blocks {bp} and {bn} do not meet along their shared face; \
                         this may be due to inconsistent grading"
                    ));
                }
                let (pa, pb) = (op + a, on + b);
                let mut min = pa.min(pb) as i64;
                let ma = list.get(pa).copied().unwrap_or(-1);
                let mb = list.get(pb).copied().unwrap_or(-1);
                if ma != -1 {
                    min = min.min(ma);
                }
                if mb != -1 {
                    min = min.min(mb);
                }
                if let Some(slot) = list.get_mut(pa) {
                    *slot = min;
                }
                if let Some(slot) = list.get_mut(pb) {
                    *slot = min;
                }
            }
        }
    }
    let n_points = renumber(&mut list)?;
    Ok(Merge {
        list: list.into_iter().map(|v| v as usize).collect(),
        n_points,
    })
}

/// Lowers the merge label of both points to the least label either holds.
fn join(list: &mut [i64], a: usize, b: usize) {
    let mut min = a.min(b) as i64;
    for p in [a, b] {
        let m = list.get(p).copied().unwrap_or(-1);
        if m != -1 {
            min = min.min(m);
        }
    }
    for p in [a, b] {
        if let Some(slot) = list.get_mut(p) {
            *slot = min;
        }
    }
}

fn dist2(a: Vec3, b: Vec3) -> f64 {
    let d = a - b;
    d.x * d.x + d.y * d.y + d.z * d.z
}

/// `blockMesh::calcGeometricalMerge` (the default): points that lie
/// on each other, on a block's own faces (edges and faces that collapse)
/// and across the faces of two blocks, are one point.
fn merge_points(blocks: &[Block], topology: &Topology, corners: &[[Vec3; 8]]) -> Result<Merge> {
    let offs = offsets(blocks);
    let total = blocks.iter().map(Block::n_points).sum::<usize>();
    let mut list = vec![-1i64; total];
    let built = &topology.built;
    let n_faces = built.owner.len();
    // For every internal face, the point of the neighbour's face each point
    // of the owner's face meets.
    let mut pairs: Vec<Vec<[i64; 4]>> = vec![Vec::new(); built.n_internal];

    for face in 0..n_faces {
        let bp = built.owner.get(face).copied().unwrap_or(0);
        let sp = topology
            .side_of(bp, face)
            .ok_or_else(|| MeshError::Invalid("a block face is not found in its block".into()))?;
        let (Some(p), Some(&op)) = (blocks.get(bp), offs.get(bp)) else {
            continue;
        };
        let (mut lo, mut hi) = (
            Vec3::new(f64::MAX, f64::MAX, f64::MAX),
            Vec3::new(f64::MIN, f64::MIN, f64::MIN),
        );
        for c in corners.get(bp).into_iter().flatten() {
            lo = Vec3::new(lo.x.min(c.x), lo.y.min(c.y), lo.z.min(c.z));
            hi = Vec3::new(hi.x.max(c.x), hi.y.max(c.y), hi.z.max(c.z));
        }
        let span = (hi - lo) * (10.0 * 1e-15);
        let merge_sqr_dist = span.x * span.x + span.y * span.y + span.z * span.z;

        let p_quads = p.side(sp);
        let at = |b: &Block, l: usize| b.points.get(l).copied().unwrap_or_default();
        let mut sqr_tol = 1e300_f64;
        for quad in &p_quads {
            for (ia, &a) in quad.iter().enumerate() {
                for (ib, &b) in quad.iter().enumerate() {
                    if ia == ib {
                        continue;
                    }
                    let d = dist2(at(p, a), at(p, b));
                    if d < merge_sqr_dist {
                        join(&mut list, a + op, b + op);
                    } else {
                        sqr_tol = sqr_tol.min(d);
                    }
                }
            }
        }
        sqr_tol /= 10.0;

        if face < built.n_internal {
            let bn = built.neighbour.get(face).copied().unwrap_or(0);
            let sn = topology.side_of(bn, face).ok_or_else(|| {
                MeshError::Invalid("a block face is not found in its block".into())
            })?;
            let (Some(n), Some(&on)) = (blocks.get(bn), offs.get(bn)) else {
                continue;
            };
            let n_quads = n.side(sn);
            if p_quads.len() != n_quads.len() {
                return invalid(format!(
                    "inconsistent number of faces between blocks {bp} and {bn}"
                ));
            }
            let mut cur: Vec<[i64; 4]> = Vec::with_capacity(p_quads.len());
            for quad in &p_quads {
                let mut cp = [-1i64; 4];
                for (c, &a) in quad.iter().enumerate() {
                    for nq in &n_quads {
                        for &b in nq {
                            if dist2(at(p, a), at(n, b)) < sqr_tol {
                                if let Some(slot) = cp.get_mut(c) {
                                    *slot = b as i64;
                                }
                                join(&mut list, a + op, b + on);
                            }
                        }
                    }
                }
                if cp.contains(&-1) {
                    return invalid(format!(
                        "inconsistent point locations between blocks {bp} and {bn}; \
                         probably due to inconsistent grading"
                    ));
                }
                cur.push(cp);
            }
            if let Some(slot) = pairs.get_mut(face) {
                *slot = cur;
            }
        }
    }

    for pass in 0.. {
        let mut changed = false;
        for face in 0..built.n_internal {
            let bp = built.owner.get(face).copied().unwrap_or(0);
            let bn = built.neighbour.get(face).copied().unwrap_or(0);
            let (Some(sp), Some(p)) = (topology.side_of(bp, face), blocks.get(bp)) else {
                continue;
            };
            let (op, on) = (
                offs.get(bp).copied().unwrap_or(0),
                offs.get(bn).copied().unwrap_or(0),
            );
            let Some(cur) = pairs.get(face) else { continue };
            for (quad, cp) in p.side(sp).iter().zip(cur) {
                for (c, &a) in quad.iter().enumerate() {
                    let (pa, nb) = (a + op, cp.get(c).copied().unwrap_or(-1));
                    if nb < 0 {
                        continue;
                    }
                    let pb = nb as usize + on;
                    let (ma, mb) = (
                        list.get(pa).copied().unwrap_or(-1),
                        list.get(pb).copied().unwrap_or(-1),
                    );
                    if ma != mb {
                        changed = true;
                        let m = ma.min(mb);
                        if let Some(slot) = list.get_mut(pa) {
                            *slot = m;
                        }
                        if let Some(slot) = list.get_mut(pb) {
                            *slot = m;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
        if pass > 100 {
            return invalid("point merging failed after the largest number of passes");
        }
    }

    let n_points = renumber(&mut list)?;
    Ok(Merge {
        list: list.into_iter().map(|v| v as usize).collect(),
        n_points,
    })
}

/// `face::centre`: the centre of a polygon, area weighted.
fn face_centre(p: &[Vec3]) -> Vec3 {
    if let [a, b, c] = p {
        return (*a + *b + *c) * (1.0 / 3.0);
    }
    let n = p.len() as f64;
    let estimate = p.iter().fold(Vec3::default(), |s, &q| s + q) / n;
    let mut sum_area = 0.0;
    let mut sum_centre = Vec3::default();
    for (i, &here) in p.iter().enumerate() {
        let next = p.get((i + 1) % p.len()).copied().unwrap_or(here);
        let three_centre = here + next + estimate;
        let area = (here - estimate).cross(next - estimate).norm();
        sum_area += area;
        sum_centre = sum_centre + three_centre * area;
    }
    if sum_area > 1e-300 {
        sum_centre / (3.0 * sum_area)
    } else {
        estimate
    }
}

/// `face::areaNormal`: the vector area of a polygon.
fn face_area_normal(p: &[Vec3]) -> Vec3 {
    let tri = |a: Vec3, b: Vec3, c: Vec3| (b - a).cross(c - a) * 0.5;
    if let [a, b, c] = p {
        return tri(*a, *b, *c);
    }
    let centre = p.iter().fold(Vec3::default(), |s, &q| s + q) / p.len() as f64;
    let mut n = Vec3::default();
    for (i, &here) in p.iter().enumerate() {
        let next = p.get((i + 1) % p.len()).copied().unwrap_or(here);
        n = n + tri(here, next, centre);
    }
    n
}

/// `blockDescriptor::check`: a block must not be inside out, and each of its
/// faces must point away from its centre (a face that has collapsed to a
/// line or a point counts as pointing away).
fn check_blocks(blocks: &[Block], verts: &[Vec3]) -> Result<()> {
    for (index, b) in blocks.iter().enumerate() {
        let at = |i: usize| {
            b.vertices
                .get(i)
                .and_then(|&v| verts.get(v))
                .copied()
                .unwrap_or_default()
        };
        let faces: Vec<Vec<Vec3>> = HEX_FACES
            .iter()
            .map(|f| f.iter().map(|&i| at(i)).collect())
            .collect();
        // `cellModel::centre`: pyramids from an estimate of the centre.
        let estimate = (0..8).fold(Vec3::default(), |s, i| s + at(i)) / 8.0;
        let (mut sum_v, mut sum_vc) = (0.0, Vec3::default());
        for f in &faces {
            let base = face_centre(f);
            let volume = (1.0 / 3.0) * face_area_normal(f).dot(estimate - base);
            sum_vc = sum_vc - (base * (3.0 / 4.0) + estimate * (1.0 / 4.0)) * volume;
            sum_v -= volume;
        }
        let centre = sum_vc / (sum_v + 1e-300);

        let mut outward = 0;
        for f in &faces {
            let normal = face_area_normal(f);
            if normal.norm() > 1e-15 {
                if (face_centre(f) - centre).dot(normal) > 0.0 {
                    outward += 1;
                }
            } else {
                outward += 1;
            }
        }
        if outward == 0 {
            return invalid(format!("block {index} is inside-out"));
        }
        if outward != faces.len() {
            return invalid(format!("block {index} has inward-pointing faces"));
        }
    }
    Ok(())
}

/// Builds the mesh a `blockMeshDict` describes.
pub fn block_mesh(dict: &Dictionary) -> Result<RawPolyMesh> {
    let scale = dict
        .scalar("scale")
        .or_else(|| dict.scalar("convertToMeters"))
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0);
    let mut names = Names::of(dict)?;
    let verts = vertices(dict, &mut names)?;
    let mut blocks = blocks(dict, verts.len(), &names)?;
    check_blocks(&blocks, &verts)?;
    let curves = edges::read(dict, &verts, &names)?;
    if dict.get("faces").is_some_and(|e| match e {
        Entry::Values(v) => v
            .first()
            .and_then(Value::as_list)
            .is_some_and(|l| !l.is_empty()),
        _ => true,
    }) {
        return invalid("block faces projected on geometry are not supported yet");
    }
    if dict.get("mergePatchPairs").is_some_and(|e| match e {
        Entry::Values(v) => v
            .first()
            .and_then(Value::as_list)
            .is_some_and(|l| !l.is_empty()),
        _ => true,
    }) {
        return invalid("`mergePatchPairs` is not supported yet");
    }
    let mut corners: Vec<[Vec3; 8]> = Vec::with_capacity(blocks.len());
    for b in &mut blocks {
        let c: [Vec3; 8] = std::array::from_fn(|i| {
            b.vertices
                .get(i)
                .and_then(|&v| verts.get(v))
                .copied()
                .unwrap_or_default()
        });
        b.create_points(&c, &curves)?;
        corners.push(c);
    }
    let patches = boundary(dict, verts.len(), &names)?;
    let topology = Topology::new(&blocks, &patches)?;
    // `fastMerge` merges the points by the blocks' topology, trusting the
    // gradings to agree; otherwise (the default) by their distance.
    let fast = matches!(
        dict.word("fastMerge"),
        Some("true" | "yes" | "on" | "y" | "t" | "1")
    ) || dict.scalar("fastMerge").is_some_and(|v| v != 0.0);
    let merge = if fast {
        merge_topology(&blocks, &topology, &corners)?
    } else {
        merge_points(&blocks, &topology, &corners)?
    };
    let offs = offsets(&blocks);

    // The points: where blocks overlap, the last one wins.
    let mut points = vec![Vec3::default(); merge.n_points];
    for (b, &off) in blocks.iter().zip(&offs) {
        for (i, &p) in b.points.iter().enumerate() {
            let to = merge.list.get(off + i).copied().unwrap_or(0);
            if let Some(slot) = points.get_mut(to) {
                *slot = p * scale;
            }
        }
    }

    // The cells, collapsed to their shape.
    let mut cells = Cells::new();
    for (b, &off) in blocks.iter().zip(&offs) {
        for k in 0..b.n[2] {
            for j in 0..b.n[1] {
                for i in 0..b.n[0] {
                    let mut labels = b.cell(i, j, k);
                    for l in &mut labels {
                        *l = merge.list.get(off + *l).copied().unwrap_or(0);
                    }
                    let shape: Shape = collapse_hex(&labels)?;
                    cells.push(&shape.faces());
                }
            }
        }
    }

    // The patch faces: those of the topology patches, then the rest.
    let topo = &topology.built;
    let mut patch_faces: Vec<Vec<Vec<usize>>> = Vec::new();
    let topo_patches: Vec<(usize, usize)> = topo
        .patches
        .iter()
        .copied()
        .chain((topo.default_patch.1 > 0).then_some(topo.default_patch))
        .collect();
    for &(start, size) in &topo_patches {
        let mut faces: Vec<Vec<usize>> = Vec::new();
        for face in start..start + size {
            let (block, side) = topology
                .locate(face)
                .ok_or_else(|| MeshError::Invalid("a patch face is on no block".into()))?;
            let (Some(b), Some(&off)) = (blocks.get(block), offs.get(block)) else {
                continue;
            };
            for quad in b.side(side) {
                let q: Vec<usize> = quad
                    .iter()
                    .map(|&p| merge.list.get(off + p).copied().unwrap_or(0))
                    .collect();
                let mut unique: Vec<usize> = Vec::with_capacity(4);
                for &v in &q {
                    if unique.last() != Some(&v) {
                        unique.push(v);
                    }
                }
                if unique.len() > 1 && unique.first() == unique.last() {
                    unique.pop();
                }
                if unique.len() >= 3 {
                    faces.push(unique);
                }
            }
        }
        patch_faces.push(faces);
    }

    let built = shapes::build(&cells, &patch_faces)?;
    let mut raw = RawPolyMesh {
        points,
        faces: FaceList {
            offsets: built.faces.offsets,
            points: built.faces.points,
        },
        owner: built.owner,
        neighbour: built.neighbour,
        patches: Vec::new(),
        cell_zones: Vec::new(),
    };
    let default_name = dict
        .dict("defaultPatch")
        .and_then(|d| d.word("name"))
        .unwrap_or("defaultFaces")
        .to_owned();
    let default_type = dict
        .dict("defaultPatch")
        .and_then(|d| d.word("type"))
        .unwrap_or("empty")
        .to_owned();
    let mut specs: Vec<(String, String, Dictionary)> = patches
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.dict.word("type").unwrap_or("patch").to_owned(),
                p.dict.clone(),
            )
        })
        .collect();
    if topo.default_patch.1 > 0 {
        specs.push((default_name, default_type, Dictionary::new()));
    }
    for ((name, kind, mut dict), &(start, size)) in specs.into_iter().zip(&built.patches) {
        dict.set("faces", Entry::Values(Vec::new()));
        let groups = match kind.as_str() {
            "wall" | "empty" | "wedge" | "symmetryPlane" | "symmetry" | "cyclic" | "cyclicSlip" => {
                vec![kind.clone()]
            }
            _ => Vec::new(),
        };
        raw.patches.push(RawPatch {
            name,
            kind,
            start_face: start,
            n_faces: size,
            groups,
            dict: strip(dict),
        });
    }
    raw.cell_zones = cell_zones(&blocks);
    Ok(raw)
}

/// The cell zones the blocks name, numbered in the order their names first
/// appear; the cells of a block are consecutive, blocks following one another.
fn cell_zones(blocks: &[Block]) -> Vec<RawCellZone> {
    let mut zones: Vec<RawCellZone> = Vec::new();
    let mut cell = 0usize;
    for b in blocks {
        let n = b.n_cells();
        if let Some(name) = &b.zone {
            let at = match zones.iter().position(|z| &z.name == name) {
                Some(at) => at,
                None => {
                    zones.push(RawCellZone {
                        name: name.clone(),
                        cells: Vec::new(),
                    });
                    zones.len() - 1
                }
            };
            if let Some(zone) = zones.get_mut(at) {
                zone.cells.extend(cell..cell + n);
            }
        }
        cell += n;
    }
    zones
}

/// A patch's own settings, without the entries a mesh file holds itself.
fn strip(dict: Dictionary) -> Dictionary {
    let mut out = Dictionary::new();
    for (key, entry) in dict.entries() {
        if !matches!(key, "type" | "faces" | "inGroups" | "nFaces" | "startFace") {
            out.insert(key, entry.clone());
        }
    }
    out
}
