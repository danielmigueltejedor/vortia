//! Cell shapes and the mesh made from them: the part of `blockMesh` that
//! decides how its cells are labelled and in which order its faces come.
//!
//! Derived from OpenFOAM's `cellMatcher`, `degenerateMatcher` and
//! `polyMesh(…, cellShapes, …)` (GPL-3.0-or-later, like Vortia): a block's
//! cell with collapsed edges is recognised as a hexahedron, prism, … by
//! walking its faces, and the mesh takes the faces of every cell in the order
//! of the shape's model, the internal ones by increasing neighbour.

use std::collections::HashMap;

use vortia_foamfile::FaceList;

use crate::error::{MeshError, Result};

/// The six faces of a hexahedron, by its vertices (OpenFOAM's `hex` model).
pub const HEX_FACES: [[usize; 4]; 6] = [
    [0, 4, 7, 3],
    [1, 2, 6, 5],
    [0, 1, 5, 4],
    [3, 7, 6, 2],
    [0, 3, 2, 1],
    [4, 5, 6, 7],
];

/// The five faces of a prism (`prism` model): bottom and top triangles, then
/// three quadrilaterals.
pub const PRISM_FACES: [&[usize]; 5] = [
    &[0, 2, 1],
    &[3, 4, 5],
    &[0, 3, 5, 2],
    &[1, 2, 5, 4],
    &[0, 1, 4, 3],
];

/// The six faces of a wedge: two triangles and four quadrilaterals.
pub const WEDGE_FACES: [&[usize]; 6] = [
    &[0, 2, 1],
    &[3, 4, 5, 6],
    &[0, 3, 6],
    &[1, 2, 5, 4],
    &[0, 1, 4, 3],
    &[0, 6, 5, 2],
];

/// The faces of a pyramid: the base, then four triangles.
pub const PYR_FACES: [&[usize]; 5] = [
    &[0, 3, 2, 1],
    &[0, 4, 3],
    &[3, 4, 2],
    &[1, 2, 4],
    &[0, 1, 4],
];

/// The four faces of a tetrahedron.
pub const TET_FACES: [&[usize]; 4] = [&[1, 2, 3], &[0, 3, 2], &[0, 1, 3], &[0, 2, 1]];

/// The faces of a tetWedge: two triangles and two quadrilaterals.
pub const TET_WEDGE_FACES: [&[usize]; 4] = [&[0, 2, 1], &[0, 1, 3], &[0, 3, 4, 2], &[1, 2, 4, 3]];

/// A cell's model, with the vertex labels it was matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Hex([usize; 8]),
    Prism([usize; 6]),
    Wedge([usize; 7]),
    Pyramid([usize; 5]),
    Tet([usize; 4]),
    TetWedge([usize; 5]),
}

impl Shape {
    /// The vertex labels the shape was given.
    pub fn labels(&self) -> &[usize] {
        match self {
            Shape::Hex(v) => v,
            Shape::Prism(v) => v,
            Shape::Wedge(v) => v,
            Shape::Pyramid(v) => v,
            Shape::Tet(v) => v,
            Shape::TetWedge(v) => v,
        }
    }

    /// The faces, in the model's order, as vertex labels.
    pub fn faces(&self) -> Vec<Vec<usize>> {
        let pick = |model: &[usize], v: &[usize]| -> Vec<usize> {
            model.iter().filter_map(|&i| v.get(i).copied()).collect()
        };
        match self {
            Shape::Hex(v) => HEX_FACES.iter().map(|f| pick(f, v)).collect(),
            Shape::Prism(v) => PRISM_FACES.iter().map(|f| pick(f, v)).collect(),
            Shape::Wedge(v) => WEDGE_FACES.iter().map(|f| pick(f, v)).collect(),
            Shape::Pyramid(v) => PYR_FACES.iter().map(|f| pick(f, v)).collect(),
            Shape::Tet(v) => TET_FACES.iter().map(|f| pick(f, v)).collect(),
            Shape::TetWedge(v) => TET_WEDGE_FACES.iter().map(|f| pick(f, v)).collect(),
        }
    }
}

/// The cell `labels` (a hexahedron's eight vertices, some of which may be
/// the same point) as the shape it collapses to.
pub fn collapse_hex(labels: &[usize; 8]) -> Result<Shape> {
    let faces = collapsed_faces(labels);
    match_shape(&faces)
}

/// `cellShape::collapsedFaces`: the hex's faces without repeated vertices,
/// and without the faces that are left with fewer than three.
fn collapsed_faces(labels: &[usize; 8]) -> Vec<Vec<usize>> {
    let mut out = Vec::with_capacity(6);
    for model in &HEX_FACES {
        let f: Vec<usize> = model
            .iter()
            .filter_map(|&i| labels.get(i).copied())
            .collect();
        let mut new: Vec<usize> = Vec::with_capacity(4);
        let mut prev = usize::MAX;
        for &v in &f {
            if v != prev {
                new.push(v);
                prev = v;
            }
        }
        if new.len() > 1 && new.first() == new.last() {
            new.pop();
        }
        if new.len() > 2 {
            out.push(new);
        }
    }
    out
}

/// What the matcher works with: the faces of one cell with vertices
/// renumbered by first appearance, and which faces meet at each edge.
struct Local {
    faces: Vec<Vec<usize>>,
    point_map: Vec<usize>,
    edge_faces: HashMap<(usize, usize), [Option<usize>; 2]>,
    /// `point_face_index[vertex][face]`: the vertex's place in the face.
    point_face_index: Vec<Vec<Option<usize>>>,
}

impl Local {
    fn new(faces: &[Vec<usize>], max_vertices: usize) -> Option<Self> {
        let mut seen: HashMap<usize, usize> = HashMap::new();
        let mut point_map: Vec<usize> = Vec::new();
        let mut local = Vec::with_capacity(faces.len());
        for f in faces {
            let mut lf = Vec::with_capacity(f.len());
            for &v in f {
                let id = match seen.get(&v) {
                    Some(&id) => id,
                    None => {
                        if point_map.len() >= max_vertices {
                            return None;
                        }
                        let id = point_map.len();
                        seen.insert(v, id);
                        point_map.push(v);
                        id
                    }
                };
                lf.push(id);
            }
            local.push(lf);
        }
        if point_map.len() != max_vertices {
            return None;
        }
        let mut edge_faces: HashMap<(usize, usize), [Option<usize>; 2]> = HashMap::new();
        for (fi, f) in local.iter().enumerate() {
            let mut prev = f.len().checked_sub(1)?;
            for fp in 0..f.len() {
                let (start, end) = (*f.get(prev)?, *f.get(fp)?);
                let slot = edge_faces.entry((start, end)).or_insert([None, None]);
                if slot[0].is_none() {
                    slot[0] = Some(fi);
                } else if slot[1].is_none() {
                    slot[1] = Some(fi);
                } else {
                    return None;
                }
                let mirror = edge_faces.entry((end, start)).or_insert([None, None]);
                if mirror[0].is_none() {
                    mirror[0] = Some(fi);
                } else if mirror[1].is_none() {
                    mirror[1] = Some(fi);
                } else {
                    return None;
                }
                prev = fp;
            }
        }
        let mut point_face_index = vec![vec![None; faces.len()]; max_vertices];
        for (fi, f) in local.iter().enumerate() {
            for (fp, &v) in f.iter().enumerate() {
                if let Some(slot) = point_face_index.get_mut(v).and_then(|r| r.get_mut(fi)) {
                    *slot = Some(fp);
                }
            }
        }
        Some(Self {
            faces: local,
            point_map,
            edge_faces,
            point_face_index,
        })
    }

    /// The face on the other side of the edge `v0 v1` from `face`.
    fn other_face(&self, v0: usize, v1: usize, face: usize) -> Option<usize> {
        let e = self.edge_faces.get(&(v0, v1))?;
        if e[0] == Some(face) {
            e[1]
        } else if e[1] == Some(face) {
            e[0]
        } else {
            None
        }
    }

    fn vertex(&self, face: usize, at: usize) -> Option<usize> {
        self.faces.get(face)?.get(at).copied()
    }

    fn size(&self, face: usize) -> usize {
        self.faces.get(face).map_or(0, Vec::len)
    }

    /// The global label of the vertex at `at` in `face`.
    fn global(&self, face: usize, at: usize) -> Option<usize> {
        self.point_map.get(self.vertex(face, at)?).copied()
    }

    /// Where the local vertex sits in `face`.
    fn place(&self, vertex: usize, face: usize) -> Option<usize> {
        self.point_face_index
            .get(vertex)?
            .get(face)
            .copied()
            .flatten()
    }
}

/// `cellMatcher::nextVert`: forward when `right_hand`, backward otherwise.
fn next_vert(at: usize, size: usize, right_hand: bool) -> usize {
    if right_hand {
        (at + 1) % size
    } else {
        (size + at - 1) % size
    }
}

/// `degenerateMatcher::match`: the model a cell's collapsed faces make,
/// tried in the order hex, tet, prism, pyramid, wedge, tetWedge.
fn match_shape(faces: &[Vec<usize>]) -> Result<Shape> {
    let found = match_hex(faces)
        .or_else(|| match_tet(faces))
        .or_else(|| match_prism(faces))
        .or_else(|| match_pyramid(faces))
        .or_else(|| match_wedge(faces))
        .or_else(|| match_tet_wedge(faces));
    found.ok_or_else(|| {
        MeshError::Invalid(format!(
            "a block cell collapses to something that is no hexahedron, prism, \
             pyramid, wedge or tetrahedron ({} faces, {} of them triangles)",
            faces.len(),
            faces.iter().filter(|f| f.len() == 3).count()
        ))
    })
}

fn count_sizes(faces: &[Vec<usize>]) -> Option<(usize, usize)> {
    let (mut tris, mut quads) = (0, 0);
    for f in faces {
        match f.len() {
            3 => tris += 1,
            4 => quads += 1,
            _ => return None,
        }
    }
    Some((tris, quads))
}

/// `tetMatcher`: four triangles.
fn match_tet(faces: &[Vec<usize>]) -> Option<Shape> {
    if faces.len() != 4 || count_sizes(faces)? != (4, 0) {
        return None;
    }
    let l = Local::new(faces, 4)?;
    let mut v = [0usize; 4];
    let face3 = 0usize;
    let size = l.size(face3);
    let v30 = 0usize;
    *v.get_mut(0)? = l.global(face3, v30)?;
    let v31 = next_vert(v30, size, false);
    *v.get_mut(1)? = l.global(face3, v31)?;
    let v32 = next_vert(v31, size, false);
    *v.get_mut(2)? = l.global(face3, v32)?;
    let (a0, a1, a2) = (
        l.vertex(face3, v30)?,
        l.vertex(face3, v31)?,
        l.vertex(face3, v32)?,
    );
    // The faces across the three edges; the last is the one that holds the
    // fourth vertex.
    l.other_face(a0, a1, face3)?;
    l.other_face(a1, a2, face3)?;
    let face1 = l.other_face(a2, a0, face3)?;
    let face1_vert0 = l.place(a0, face1)?;
    let face1_vert3 = next_vert(face1_vert0, l.size(face1), true);
    *v.get_mut(3)? = l.global(face1, face1_vert3)?;
    Some(Shape::Tet(v))
}

/// `pyrMatcher`: a quadrilateral and four triangles.
fn match_pyramid(faces: &[Vec<usize>]) -> Option<Shape> {
    if faces.len() != 5 || count_sizes(faces)? != (4, 1) {
        return None;
    }
    let l = Local::new(faces, 5)?;
    let mut v = [0usize; 5];
    let face0 = faces.iter().position(|f| f.len() == 4)?;
    let size0 = l.size(face0);
    let v00 = 0usize;
    *v.get_mut(0)? = l.global(face0, v00)?;
    let v01 = next_vert(v00, size0, false);
    *v.get_mut(1)? = l.global(face0, v01)?;
    let v02 = next_vert(v01, size0, false);
    *v.get_mut(2)? = l.global(face0, v02)?;
    let v03 = next_vert(v02, size0, false);
    *v.get_mut(3)? = l.global(face0, v03)?;
    let (b0, b1, b2, b3) = (
        l.vertex(face0, v00)?,
        l.vertex(face0, v01)?,
        l.vertex(face0, v02)?,
        l.vertex(face0, v03)?,
    );
    l.other_face(b3, b0, face0)?;
    l.other_face(b2, b3, face0)?;
    l.other_face(b1, b2, face0)?;
    let face4 = l.other_face(b0, b1, face0)?;
    let face4_vert0 = l.place(b0, face4)?;
    let face4_vert4 = next_vert(face4_vert0, l.size(face4), false);
    *v.get_mut(4)? = l.global(face4, face4_vert4)?;
    Some(Shape::Pyramid(v))
}

/// `wedgeMatcher`: two triangles and four quadrilaterals, tried from each
/// rotation of the first triangle.
fn match_wedge(faces: &[Vec<usize>]) -> Option<Shape> {
    if faces.len() != 6 || count_sizes(faces)? != (2, 4) {
        return None;
    }
    let l = Local::new(faces, 7)?;
    let face0 = faces.iter().position(|f| f.len() == 3)?;
    (0..l.size(face0)).find_map(|v00| wedge_from(&l, face0, v00))
}

fn wedge_from(l: &Local, face0: usize, v00: usize) -> Option<Shape> {
    let mut v = [0usize; 7];
    let size0 = l.size(face0);
    *v.get_mut(0)? = l.global(face0, v00)?;
    let v01 = next_vert(v00, size0, false);
    *v.get_mut(1)? = l.global(face0, v01)?;
    let (a0, a1) = (l.vertex(face0, v00)?, l.vertex(face0, v01)?);
    let face4 = l.other_face(a0, a1, face0)?;
    if l.size(face4) != 4 {
        return None;
    }
    let face4_vert0 = l.place(a0, face4)?;
    let face4_vert3 = next_vert(face4_vert0, l.size(face4), false);
    *v.get_mut(3)? = l.global(face4, face4_vert3)?;
    let (c0, c3) = (l.vertex(face4, face4_vert0)?, l.vertex(face4, face4_vert3)?);
    let face2 = l.other_face(c0, c3, face4)?;
    if l.size(face2) != 3 {
        return None;
    }
    let face2_vert3 = l.place(c3, face2)?;
    let face2_vert6 = next_vert(face2_vert3, l.size(face2), true);
    *v.get_mut(6)? = l.global(face2, face2_vert6)?;
    let (d3, d6) = (l.vertex(face2, face2_vert3)?, l.vertex(face2, face2_vert6)?);
    let face1 = l.other_face(d3, d6, face2)?;
    let face1_vert6 = l.place(d6, face1)?;
    let face1_vert5 = next_vert(face1_vert6, l.size(face1), false);
    *v.get_mut(5)? = l.global(face1, face1_vert5)?;
    let face1_vert4 = next_vert(face1_vert5, l.size(face1), false);
    *v.get_mut(4)? = l.global(face1, face1_vert4)?;
    let v02 = next_vert(v01, size0, false);
    *v.get_mut(2)? = l.global(face0, v02)?;
    let a2 = l.vertex(face0, v02)?;
    l.other_face(a1, a2, face0)?;
    l.other_face(a2, a0, face0)?;
    Some(Shape::Wedge(v))
}

/// `tetWedgeMatcher`: two triangles and two quadrilaterals.
fn match_tet_wedge(faces: &[Vec<usize>]) -> Option<Shape> {
    if faces.len() != 4 || count_sizes(faces)? != (2, 2) {
        return None;
    }
    let l = Local::new(faces, 5)?;
    let face0 = faces.iter().position(|f| f.len() == 3)?;
    (0..l.size(face0)).find_map(|v00| tet_wedge_from(&l, face0, v00))
}

fn tet_wedge_from(l: &Local, face0: usize, v00: usize) -> Option<Shape> {
    let mut v = [0usize; 5];
    let size0 = l.size(face0);
    *v.get_mut(0)? = l.global(face0, v00)?;
    let v01 = next_vert(v00, size0, false);
    *v.get_mut(1)? = l.global(face0, v01)?;
    let (a0, a1) = (l.vertex(face0, v00)?, l.vertex(face0, v01)?);
    let face1 = l.other_face(a0, a1, face0)?;
    if l.size(face1) != 3 {
        return None;
    }
    let v02 = next_vert(v01, size0, false);
    *v.get_mut(2)? = l.global(face0, v02)?;
    let a2 = l.vertex(face0, v02)?;
    let face3 = l.other_face(a1, a2, face0)?;
    l.other_face(a2, a0, face0)?;
    let face3_vert2 = l.place(a2, face3)?;
    let size3 = l.size(face3);
    let face3_vert4 = next_vert(face3_vert2, size3, true);
    *v.get_mut(4)? = l.global(face3, face3_vert4)?;
    let face3_vert3 = next_vert(face3_vert4, size3, true);
    *v.get_mut(3)? = l.global(face3, face3_vert3)?;
    Some(Shape::TetWedge(v))
}

fn match_hex(faces: &[Vec<usize>]) -> Option<Shape> {
    if faces.len() != 6 || faces.iter().any(|f| f.len() != 4) {
        return None;
    }
    let l = Local::new(faces, 8)?;
    // A cell is the owner of all its faces here, so the faces run
    // anticlockwise seen from outside: `next_vert(.., false)` walks them
    // against their order, `next_vert(.., true)` along it.
    let mut v = [0usize; 8];
    let face4 = 0usize;
    let face4_vert0 = 0usize;
    *v.get_mut(0)? = l.global(face4, face4_vert0)?;
    let size4 = l.size(face4);
    let face4_vert1 = next_vert(face4_vert0, size4, false);
    *v.get_mut(1)? = l.global(face4, face4_vert1)?;
    let face4_vert2 = next_vert(face4_vert1, size4, false);
    *v.get_mut(2)? = l.global(face4, face4_vert2)?;
    let face4_vert3 = next_vert(face4_vert2, size4, false);
    *v.get_mut(3)? = l.global(face4, face4_vert3)?;

    let (f40, f41, f42, f43) = (
        l.vertex(face4, face4_vert0)?,
        l.vertex(face4, face4_vert1)?,
        l.vertex(face4, face4_vert2)?,
        l.vertex(face4, face4_vert3)?,
    );
    // Jump the edge from face 4 to face 0.
    let face0 = l.other_face(f43, f40, face4)?;
    let face0_vert0 = l.place(f40, face0)?;
    let size0 = l.size(face0);
    let face0_vert4 = next_vert(face0_vert0, size0, true);
    *v.get_mut(4)? = l.global(face0, face0_vert4)?;
    let face0_vert7 = next_vert(face0_vert4, size0, true);
    *v.get_mut(7)? = l.global(face0, face0_vert7)?;
    // From face 0 to face 5.
    let (f04, f07) = (l.vertex(face0, face0_vert4)?, l.vertex(face0, face0_vert7)?);
    let face5 = l.other_face(f04, f07, face0)?;
    let face5_vert4 = l.place(f04, face5)?;
    let size5 = l.size(face5);
    let face5_vert5 = next_vert(face5_vert4, size5, true);
    *v.get_mut(5)? = l.global(face5, face5_vert5)?;
    let face5_vert6 = next_vert(face5_vert5, size5, true);
    *v.get_mut(6)? = l.global(face5, face5_vert6)?;
    // The other faces are fixed by these, and so are not looked for.
    let _ = (f41, f42);
    Some(Shape::Hex(v))
}

fn match_prism(faces: &[Vec<usize>]) -> Option<Shape> {
    let tris = faces.iter().filter(|f| f.len() == 3).count();
    let quads = faces.iter().filter(|f| f.len() == 4).count();
    if faces.len() != 5 || tris != 2 || quads != 3 {
        return None;
    }
    let l = Local::new(faces, 6)?;
    let mut v = [0usize; 6];
    let face0 = faces.iter().position(|f| f.len() == 3)?;
    let face0_vert0 = 0usize;
    *v.get_mut(0)? = l.global(face0, face0_vert0)?;
    let size0 = l.size(face0);
    let face0_vert1 = next_vert(face0_vert0, size0, false);
    *v.get_mut(1)? = l.global(face0, face0_vert1)?;
    let (a0, a1) = (l.vertex(face0, face0_vert0)?, l.vertex(face0, face0_vert1)?);
    // Jump the edge from face 0 to face 4.
    let face4 = l.other_face(a0, a1, face0)?;
    if l.size(face4) != 4 {
        return None;
    }
    let face4_vert1 = l.place(a1, face4)?;
    let size4 = l.size(face4);
    let face4_vert4 = next_vert(face4_vert1, size4, true);
    *v.get_mut(4)? = l.global(face4, face4_vert4)?;
    let face4_vert3 = next_vert(face4_vert4, size4, true);
    *v.get_mut(3)? = l.global(face4, face4_vert3)?;
    let (b3, b4) = (l.vertex(face4, face4_vert3)?, l.vertex(face4, face4_vert4)?);
    // Jump the edge from face 4 to face 1, the other triangle.
    let face1 = l.other_face(b3, b4, face4)?;
    if l.size(face1) != 3 {
        return None;
    }
    let face0_vert2 = next_vert(face0_vert1, size0, false);
    *v.get_mut(2)? = l.global(face0, face0_vert2)?;
    let a2 = l.vertex(face0, face0_vert2)?;
    // Jump the edge from face 0 to face 3.
    let face3 = l.other_face(a1, a2, face0)?;
    let face3_vert2 = l.place(a2, face3)?;
    let size3 = l.size(face3);
    let face3_vert5 = next_vert(face3_vert2, size3, true);
    *v.get_mut(5)? = l.global(face3, face3_vert5)?;
    Some(Shape::Prism(v))
}

/// The cells of a mesh as their faces, in the order of each cell's model.
#[derive(Debug, Default)]
pub struct Cells {
    /// `cell_start[c]..cell_start[c + 1]` are cell `c`'s faces among `face_start`.
    pub cell_start: Vec<usize>,
    /// `points[face_start[i]..face_start[i + 1]]` is face `i`.
    pub face_start: Vec<usize>,
    pub points: Vec<usize>,
}

impl Cells {
    pub fn new() -> Self {
        Self {
            cell_start: vec![0],
            face_start: vec![0],
            points: Vec::new(),
        }
    }

    pub fn push(&mut self, faces: &[Vec<usize>]) {
        for f in faces {
            self.points.extend_from_slice(f);
            self.face_start.push(self.points.len());
        }
        self.cell_start.push(self.face_start.len() - 1);
    }

    pub fn len(&self) -> usize {
        self.cell_start.len().saturating_sub(1)
    }

    fn faces_of(&self, cell: usize) -> std::ops::Range<usize> {
        match (self.cell_start.get(cell), self.cell_start.get(cell + 1)) {
            (Some(&a), Some(&b)) => a..b,
            _ => 0..0,
        }
    }

    fn face(&self, index: usize) -> &[usize] {
        match (self.face_start.get(index), self.face_start.get(index + 1)) {
            (Some(&a), Some(&b)) => self.points.get(a..b).unwrap_or_default(),
            _ => &[],
        }
    }
}

/// The mesh the cells make.
#[derive(Debug, Default)]
pub struct Built {
    pub faces: FaceList,
    pub owner: Vec<usize>,
    pub neighbour: Vec<usize>,
    pub n_internal: usize,
    /// First face and number of faces of each patch given.
    pub patches: Vec<(usize, usize)>,
    /// The faces on no patch.
    pub default_patch: (usize, usize),
    /// The mesh face of each cell face (`Cells`' numbering).
    pub cell_face: Vec<usize>,
}

/// The faces' vertices as an unordered key, so that the faces of two cells
/// that coincide land in the same bucket.
fn key(face: &[usize]) -> (usize, [usize; 4]) {
    let mut k = [usize::MAX; 4];
    for (slot, &v) in k.iter_mut().zip(face) {
        *slot = v;
    }
    k.sort_unstable();
    (face.len(), k)
}

/// `face::operator==`: the same vertices, in the same order or the reverse.
fn same_face(a: &[usize], b: &[usize]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let Some(start) = b.iter().position(|&v| Some(&v) == a.first()) else {
        return false;
    };
    let n = a.len();
    let at = |i: usize| b.get(i % n).copied();
    let forward = (0..n).all(|i| a.get(i).copied() == at(start + i));
    let backward = (0..n).all(|i| a.get(i).copied() == at(start + n - i));
    forward || backward
}

/// The faces of `cells` ordered as OpenFOAM's `polyMesh` constructor does:
/// each cell's faces to higher cells by increasing neighbour, then the faces
/// of each patch in the order given, then the rest.
pub fn build(cells: &Cells, patches: &[Vec<Vec<usize>>]) -> Result<Built> {
    let n_cells = cells.len();
    let n_cell_faces = cells.face_start.len().saturating_sub(1);
    let mut buckets: HashMap<(usize, [usize; 4]), Vec<usize>> = HashMap::new();
    for fi in 0..n_cell_faces {
        buckets.entry(key(cells.face(fi))).or_default().push(fi);
    }
    // Which cell a face (in `Cells`' numbering) belongs to.
    let mut cell_of = vec![0usize; n_cell_faces];
    for c in 0..n_cells {
        for fi in cells.faces_of(c) {
            if let Some(slot) = cell_of.get_mut(fi) {
                *slot = c;
            }
        }
    }

    let mut cell_face = vec![usize::MAX; n_cell_faces];
    let mut out = Built::default();
    out.faces.offsets.push(0);
    let push_face = |out: &mut Built, f: &[usize], owner: usize, neighbour: Option<usize>| {
        out.faces.points.extend_from_slice(f);
        out.faces.offsets.push(out.faces.points.len());
        out.owner.push(owner);
        if let Some(n) = neighbour {
            out.neighbour.push(n);
        }
    };

    for c in 0..n_cells {
        let range = cells.faces_of(c);
        let mut neighbours: Vec<Option<(usize, usize)>> = vec![None; range.len()];
        let mut n_neighbours = 0;
        for (local, fi) in range.clone().enumerate() {
            if cell_face.get(fi).is_some_and(|&m| m != usize::MAX) {
                continue;
            }
            let face = cells.face(fi);
            let Some(bucket) = buckets.get(&key(face)) else {
                continue;
            };
            // Cells are numbered along `bucket`, so the first match is the
            // lowest cell above this one.
            let found = bucket.iter().copied().find(|&other| {
                cell_of.get(other).is_some_and(|&oc| oc > c) && same_face(face, cells.face(other))
            });
            if let Some(other) = found
                && let Some(slot) = neighbours.get_mut(local)
            {
                *slot = Some((cell_of.get(other).copied().unwrap_or(0), other));
                n_neighbours += 1;
            }
        }
        for _ in 0..n_neighbours {
            let mut next: Option<usize> = None;
            let mut min = n_cells;
            for (local, n) in neighbours.iter().enumerate() {
                if let Some((nc, _)) = n
                    && *nc < min
                {
                    next = Some(local);
                    min = *nc;
                }
            }
            let Some(local) = next else {
                return Err(MeshError::Invalid(
                    "a block cell's internal faces could not be ordered".into(),
                ));
            };
            let Some((nc, nfi)) = neighbours.get(local).copied().flatten() else {
                break;
            };
            let fi = range.start + local;
            let mesh_face = out.owner.len();
            push_face(&mut out, cells.face(fi), c, Some(nc));
            if let Some(slot) = cell_face.get_mut(fi) {
                *slot = mesh_face;
            }
            if let Some(slot) = cell_face.get_mut(nfi) {
                *slot = mesh_face;
            }
            if let Some(slot) = neighbours.get_mut(local) {
                *slot = None;
            }
        }
    }
    out.n_internal = out.owner.len();

    for (patch, patch_faces) in patches.iter().enumerate() {
        let start = out.owner.len();
        for face in patch_faces {
            let bucket = buckets.get(&key(face)).ok_or_else(|| {
                MeshError::Invalid(format!(
                    "face {face:?} of patch {patch} does not belong to any cell"
                ))
            })?;
            let fi = bucket
                .iter()
                .copied()
                .find(|&fi| same_vertices(cells.face(fi), face))
                .ok_or_else(|| {
                    MeshError::Invalid(format!(
                        "face {face:?} of patch {patch} does not belong to any cell"
                    ))
                })?;
            if cell_face.get(fi).is_some_and(|&m| m != usize::MAX) {
                return Err(MeshError::Invalid(format!(
                    "face {face:?} of patch {patch} is on an internal face or on a face of another patch"
                )));
            }
            let mesh_face = out.owner.len();
            push_face(
                &mut out,
                cells.face(fi),
                cell_of.get(fi).copied().unwrap_or(0),
                None,
            );
            if let Some(slot) = cell_face.get_mut(fi) {
                *slot = mesh_face;
            }
        }
        out.patches.push((start, out.owner.len() - start));
    }

    let default_start = out.owner.len();
    for c in 0..n_cells {
        for fi in cells.faces_of(c) {
            if cell_face.get(fi).is_some_and(|&m| m == usize::MAX) {
                let mesh_face = out.owner.len();
                push_face(&mut out, cells.face(fi), c, None);
                if let Some(slot) = cell_face.get_mut(fi) {
                    *slot = mesh_face;
                }
            }
        }
    }
    out.default_patch = (default_start, out.owner.len() - default_start);
    out.cell_face = cell_face;
    Ok(out)
}

/// `face::sameVertices`: the same set of vertices.
fn same_vertices(a: &[usize], b: &[usize]) -> bool {
    a.len() == b.len() && a.iter().all(|v| b.contains(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hex_without_collapsed_points_is_relabelled_from_its_first_face() {
        let labels = [10, 11, 12, 13, 14, 15, 16, 17];
        let Shape::Hex(v) = collapse_hex(&labels).unwrap() else {
            panic!("not a hex")
        };
        // Still the same eight points.
        let mut sorted = v;
        sorted.sort_unstable();
        assert_eq!(sorted, labels);
    }

    #[test]
    fn two_collapsed_edges_make_a_prism() {
        // z-edges 0-4 and 1-5 collapsed.
        let labels = [0, 1, 2, 3, 0, 1, 6, 7];
        assert!(matches!(collapse_hex(&labels), Ok(Shape::Prism(_))));
    }

    #[test]
    fn two_cells_share_one_internal_face() {
        let mut cells = Cells::new();
        let a = Shape::Hex([0, 1, 2, 3, 4, 5, 6, 7]).faces();
        let b = Shape::Hex([1, 8, 9, 2, 5, 10, 11, 6]).faces();
        cells.push(&a);
        cells.push(&b);
        let built = build(&cells, &[]).unwrap();
        assert_eq!(built.n_internal, 1);
        assert_eq!(built.owner.len(), 11);
        assert_eq!(built.neighbour, vec![1]);
        assert_eq!(built.default_patch, (1, 10));
    }
}
