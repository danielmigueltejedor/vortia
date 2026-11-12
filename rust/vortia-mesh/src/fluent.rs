//! The engine's own `fluentMeshToFoam`: a Fluent `.msh` file (ASCII) as the
//! files of a `polyMesh`.
//!
//! Derived from OpenFOAM's `fluentMeshToFoam` (GPL-3.0-or-later, like
//! Vortia), so that the same file gives the same mesh, point for point and
//! face for face, quirks included. The file's points, faces (with the two
//! cells each one separates), cell types and zone names are read; every cell
//! is then recognised as a tetrahedron, hexahedron, pyramid or prism from
//! the faces that belong to it (a triangle or quadrilateral of a 2-D mesh is
//! extruded into a prism or hexahedron with an `empty` front and back), the
//! mesh is built from those shapes, and the boundary faces are put on the
//! patches the zones name, found among the mesh's faces by their vertices.
//! A zone of an inside type (`interior`, `interface`, ...) is no patch; a
//! boundary face no zone claims goes to `default_wall`.

use std::collections::HashMap;

use vortia_core::Vec3;
use vortia_foamfile::{Dictionary, FaceList, RawCellZone, RawFaceZone, RawPatch, RawPolyMesh};

use crate::blockmesh::shapes::{Cells, HEX_FACES, PRISM_FACES, PYR_FACES, Shape, TET_FACES, build};
use crate::error::{MeshError, Result};
use crate::gmsh::face_points;

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

/// What the file holds, as OpenFOAM's reader gathers it.
#[derive(Default)]
struct Fluent {
    dimension: i64,
    points: Vec<Vec3>,
    faces: Vec<Vec<usize>>,
    /// Fluent's second cell of a face, minus one (`-1` for a boundary face):
    /// OpenFOAM's `owner`.
    owner: Vec<i64>,
    /// Fluent's first cell of a face, minus one: OpenFOAM's `neighbour`.
    neighbour: Vec<i64>,
    /// Fluent's model number of each cell.
    model: Vec<i64>,
    /// Zone, first and last face (counted from one) of each group of faces.
    face_groups: Vec<(i64, i64, i64)>,
    /// Zone, first and last cell (counted from one) of each group of cells.
    cell_groups: Vec<(i64, i64, i64)>,
    zone_type: HashMap<i64, String>,
    zone_name: HashMap<i64, String>,
}

/// A position in the text.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        self.text.get(self.at..).unwrap_or("")
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    /// Past the next `(`, or the end of the text.
    fn open(&mut self) -> bool {
        match self.rest().find('(') {
            Some(i) => {
                self.at += i + 1;
                true
            }
            None => {
                self.at = self.text.len();
                false
            }
        }
    }

    /// The decimal digits at the cursor.
    fn digits(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let n = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        (n > 0).then(|| {
            self.at += n;
            rest.get(..n).unwrap_or("")
        })
    }

    /// Past the `)` that closes the one already opened, skipping the quoted
    /// strings of comments and what is nested.
    fn skip_block(&mut self) {
        let mut depth = 1usize;
        let mut quoted = false;
        for (i, c) in self.rest().char_indices() {
            match c {
                '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        self.at += i + 1;
                        return;
                    }
                }
                _ => {}
            }
        }
        self.at = self.text.len();
    }

    /// The words of a parenthesised header, `(a b c)`, if one starts here.
    fn header(&mut self) -> Option<Vec<&'a str>> {
        self.skip_space();
        if self.peek() != Some('(') {
            return None;
        }
        self.at += 1;
        let rest = self.rest();
        let end = rest.find(')').unwrap_or(rest.len());
        let words = rest.get(..end).unwrap_or("").split_whitespace().collect();
        self.at += (end + 1).min(rest.len());
        Some(words)
    }

    /// The lines of a data block, `( ... )`, if one starts here.
    fn data(&mut self) -> Option<Vec<&'a str>> {
        self.skip_space();
        if self.peek() != Some('(') {
            return None;
        }
        self.at += 1;
        let rest = self.rest();
        let end = rest.find(')').unwrap_or(rest.len());
        self.at += (end + 1).min(rest.len());
        Some(rest.get(..end).unwrap_or("").lines().collect())
    }
}

fn hex(token: &str) -> Result<i64> {
    i64::from_str_radix(token, 16)
        .or_else(|_| invalid(format!("`{token}` is not a hexadecimal number")))
}

fn decimal(token: &str) -> Result<i64> {
    token
        .parse::<i64>()
        .or_else(|_| invalid(format!("`{token}` is not a whole number")))
}

fn scalar(token: &str) -> Result<f64> {
    match token.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => invalid(format!("`{token}` is not a coordinate")),
    }
}

/// The word of a zone's type or name: what OpenFOAM's `word` keeps.
fn validate(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '/' | ';' | '{' | '}'))
        .collect()
}

/// A count a header declares, which no file could back with less text than
/// it has bytes.
fn declared(token: &str, text: &str, what: &str) -> Result<usize> {
    let n = hex(token)?;
    match usize::try_from(n) {
        Ok(n) if n <= text.len() => Ok(n),
        _ => invalid(format!(
            "the file declares {n} {what}, more than it can hold"
        )),
    }
}

/// The first and last index of a group, as a range of indices counted from
/// zero, if it fits `len` things.
fn range(first: i64, last: i64, len: usize, what: &str) -> Result<std::ops::Range<usize>> {
    let a = usize::try_from(first.saturating_sub(1)).ok();
    let b = usize::try_from(last).ok();
    match (a, b) {
        (Some(a), Some(b)) if a <= b && b <= len => Ok(a..b),
        _ => invalid(format!(
            "{what} {first} to {last} do not fit the {len} declared"
        )),
    }
}

fn read(text: &str) -> Result<Fluent> {
    let mut f = Fluent::default();
    let mut c = Cursor { text, at: 0 };
    while c.open() {
        c.skip_space();
        let Some(number) = c.digits() else {
            c.skip_block();
            continue;
        };
        match number {
            "2" => {
                c.skip_space();
                f.dimension = c.digits().map_or(Ok(0), decimal)?;
                c.skip_block();
            }
            "10" => read_points(&mut c, &mut f)?,
            "13" => read_faces(&mut c, &mut f)?,
            "12" => read_cells(&mut c, &mut f)?,
            "39" | "45" => read_zone(&mut c, &mut f)?,
            _ => c.skip_block(),
        }
    }
    Ok(f)
}

fn read_points(c: &mut Cursor<'_>, f: &mut Fluent) -> Result<()> {
    let Some(head) = c.header() else {
        c.skip_block();
        return Ok(());
    };
    let (Some(zone), Some(first), Some(last)) = (head.first(), head.get(1), head.get(2)) else {
        return invalid("a points section has no first and last point");
    };
    if hex(zone)? == 0 {
        let n = declared(last, c.text, "points")?;
        f.points = vec![Vec3::default(); n];
        c.skip_block();
        return Ok(());
    }
    let wanted = range(hex(first)?, hex(last)?, f.points.len(), "points")?;
    let components = match head.get(4) {
        Some(t) => decimal(t)?,
        None => 3,
    };
    let lines = c.data().unwrap_or_default();
    let mut lines = lines.into_iter().filter(|l| !l.trim().is_empty());
    for point in wanted {
        let Some(line) = lines.next() else {
            return invalid(format!(
                "the file ends inside the points, at point {}",
                point + 1
            ));
        };
        let mut words = line.split_whitespace();
        let mut next = || match words.next() {
            Some(w) => scalar(w),
            None => invalid(format!("point {} has too few coordinates", point + 1)),
        };
        let x = next()?;
        let y = next()?;
        let z = if components == 2 { 0.0 } else { next()? };
        if let Some(p) = f.points.get_mut(point) {
            *p = Vec3::new(x, y, z);
        }
    }
    c.skip_block();
    Ok(())
}

fn read_faces(c: &mut Cursor<'_>, f: &mut Fluent) -> Result<()> {
    let Some(head) = c.header() else {
        c.skip_block();
        return Ok(());
    };
    let (Some(zone), Some(first), Some(last)) = (head.first(), head.get(1), head.get(2)) else {
        return invalid("a faces section has no first and last face");
    };
    if hex(zone)? == 0 {
        let n = declared(last, c.text, "faces")?;
        f.faces = vec![Vec::new(); n];
        f.owner = vec![-1; n];
        f.neighbour = vec![-1; n];
        c.skip_block();
        return Ok(());
    }
    let (zone, first, last) = (hex(zone)?, hex(first)?, hex(last)?);
    let wanted = range(first, last, f.faces.len(), "faces")?;
    f.face_groups.push((zone, first, last));
    let kind = match head.get(4) {
        Some(t) => hex(t)?,
        None => return invalid("a faces section has no face type"),
    };
    let lines = c.data().unwrap_or_default();
    let mut lines = lines.into_iter().filter(|l| !l.trim().is_empty());
    for face in wanted {
        let Some(line) = lines.next() else {
            return invalid(format!(
                "the file ends inside the faces, at face {}",
                face + 1
            ));
        };
        let mut words = line.split_whitespace();
        let size = if kind == 0 {
            match words.next() {
                Some(w) => hex(w)?,
                None => return invalid(format!("face {} is empty", face + 1)),
            }
        } else {
            kind
        };
        let size = usize::try_from(size)
            .ok()
            .filter(|&s| s <= line.len())
            .ok_or_else(|| MeshError::Invalid(format!("face {} has a bad size", face + 1)))?;
        let mut labels = Vec::with_capacity(size);
        for _ in 0..size {
            let Some(w) = words.next() else {
                return invalid(format!("face {} has too few points", face + 1));
            };
            labels.push(
                usize::try_from(hex(w)? - 1)
                    .or_else(|_| invalid(format!("face {} has a point 0", face + 1)))?,
            );
        }
        let (Some(a), Some(b)) = (words.next(), words.next()) else {
            return invalid(format!("face {} has no cells", face + 1));
        };
        // Fluent's first cell is OpenFOAM's neighbour and its second the owner.
        if let (Some(v), Some(n), Some(o)) = (
            f.faces.get_mut(face),
            f.neighbour.get_mut(face),
            f.owner.get_mut(face),
        ) {
            *v = labels;
            *n = hex(a)? - 1;
            *o = hex(b)? - 1;
        }
    }
    c.skip_block();
    Ok(())
}

fn read_cells(c: &mut Cursor<'_>, f: &mut Fluent) -> Result<()> {
    let Some(head) = c.header() else {
        c.skip_block();
        return Ok(());
    };
    let (Some(zone), Some(first), Some(last)) = (head.first(), head.get(1), head.get(2)) else {
        return invalid("a cells section has no first and last cell");
    };
    if hex(zone)? == 0 {
        let n = declared(last, c.text, "cells")?;
        f.model = vec![0; n];
        c.skip_block();
        return Ok(());
    }
    let (zone, first, last) = (hex(zone)?, hex(first)?, hex(last)?);
    let wanted = range(first, last, f.model.len(), "cells")?;
    f.cell_groups.push((zone, first, last));
    // Tgrid leaves the element type out: triangles in 2-D, else tetrahedra.
    let element = match head.get(4) {
        Some(t) => hex(t)?,
        None if f.dimension == 2 => 1,
        None => 2,
    };
    if element != 0 {
        for cell in wanted {
            if let Some(m) = f.model.get_mut(cell) {
                *m = element;
            }
        }
        c.skip_block();
        return Ok(());
    }
    let lines = c.data().unwrap_or_default();
    for (cell, word) in (wanted.start..).zip(lines.iter().flat_map(|l| l.split_whitespace())) {
        let m = decimal(word)?;
        match f.model.get_mut(cell) {
            Some(slot) => *slot = m,
            None => return invalid("the mixed cell types are more than the cells"),
        }
    }
    c.skip_block();
    Ok(())
}

fn read_zone(c: &mut Cursor<'_>, f: &mut Fluent) -> Result<()> {
    if let Some(head) = c.header()
        && let (Some(id), Some(kind), Some(name)) = (head.first(), head.get(1), head.get(2))
    {
        let id = decimal(id)?;
        f.zone_type.insert(id, validate(kind));
        f.zone_name.insert(id, validate(name));
    }
    c.skip_block();
    Ok(())
}

/// The faces a cell has, by Fluent's model number (the number of the
/// element type: 1 triangle, 2 tetrahedron, 3 quadrilateral, 4 hexahedron,
/// 5 pyramid, 6 prism).
fn faces_of_model(model: i64) -> Option<usize> {
    match model {
        1 => Some(3),
        2 | 3 => Some(4),
        4 => Some(6),
        5 | 6 => Some(5),
        _ => None,
    }
}

/// The faces of a cell as seen from inside it: each of the cell's faces as
/// it is, or turned round when the cell is the face's neighbour.
fn outward_faces(f: &Fluent, cell: usize, faces: &[usize]) -> Result<Vec<Vec<usize>>> {
    let me = i64::try_from(cell).unwrap_or(-1);
    let mut out = Vec::with_capacity(faces.len());
    for &face in faces {
        let (Some(v), Some(&owner), Some(&neighbour)) =
            (f.faces.get(face), f.owner.get(face), f.neighbour.get(face))
        else {
            return invalid(format!("face {face} does not exist"));
        };
        if owner == me {
            out.push(v.clone());
        } else if neighbour == me {
            out.push(v.iter().rev().copied().collect());
        } else {
            return invalid(format!(
                "face {} does not belong to cell {cell}. Face owner: {owner} neighbour: {neighbour}",
                face + 1
            ));
        }
    }
    Ok(out)
}

/// A model's faces and the order OpenFOAM matches them in.
type Model = (&'static [&'static [usize]], &'static [usize], usize);

fn model_of(fluent_model: i64) -> Option<Model> {
    const HEX: [&[usize]; 6] = [
        &HEX_FACES[0],
        &HEX_FACES[1],
        &HEX_FACES[2],
        &HEX_FACES[3],
        &HEX_FACES[4],
        &HEX_FACES[5],
    ];
    match fluent_model {
        2 => Some((&TET_FACES, &[0, 1, 2, 3], 4)),
        4 => Some((&HEX, &[0, 2, 4, 3, 5, 1], 8)),
        5 => Some((&PYR_FACES, &[0, 1, 2, 3, 4], 5)),
        6 => Some((&PRISM_FACES, &[0, 2, 3, 4, 1], 6)),
        _ => None,
    }
}

/// `create3DCellShape`: the model's vertices from the cell's faces. The first
/// face of the matching order is taken as it comes; each next one is rotated
/// until two of its points are points already placed.
fn recognise(cell: usize, local: &[Vec<usize>], fluent_model: i64) -> Result<Shape> {
    let Some((model_faces, order, n_points)) = model_of(fluent_model) else {
        return invalid(format!("unrecognised 3-D cell shape: {fluent_model}"));
    };
    let mut labels: Vec<Option<usize>> = vec![None; n_points];
    let mut used = vec![false; local.len()];
    let model_face = |k: usize| -> Result<&'static [usize]> {
        order
            .get(k)
            .and_then(|&m| model_faces.get(m))
            .copied()
            .ok_or_else(|| MeshError::Invalid("a model has no such face".into()))
    };

    let first = model_face(0)?;
    let Some(at) = local.iter().position(|f| f.len() == first.len()) else {
        return invalid(format!(
            "cell {cell}: cannot find a match for the first face"
        ));
    };
    for (m, &v) in first
        .iter()
        .zip(local.get(at).map(Vec::as_slice).unwrap_or(&[]))
    {
        if let Some(slot) = labels.get_mut(*m) {
            *slot = Some(v);
        }
    }
    if let Some(u) = used.get_mut(at) {
        *u = true;
    }

    for k in 1..order.len() {
        let wanted = model_face(k)?;
        let mut found = false;
        for (i, face) in local.iter().enumerate() {
            if found {
                break;
            }
            if used.get(i).copied().unwrap_or(true) || face.len() != wanted.len() {
                continue;
            }
            let mut rotated = face.clone();
            for _ in 0..rotated.len() {
                let matched = wanted
                    .iter()
                    .zip(&rotated)
                    .filter(|&(&m, &v)| labels.get(m).copied().flatten() == Some(v))
                    .count();
                if matched >= 2 {
                    for (&m, &v) in wanted.iter().zip(&rotated) {
                        if let Some(slot) = labels.get_mut(m) {
                            *slot = Some(v);
                        }
                    }
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    found = true;
                    break;
                }
                rotated.rotate_left(1);
            }
        }
        if !found {
            return invalid(format!(
                "cell {cell}: cannot find a match for face {k} of the model"
            ));
        }
    }

    let mut all = Vec::with_capacity(n_points);
    for l in labels {
        match l {
            Some(v) => all.push(v),
            None => return invalid(format!("cell {cell}: a vertex of the model was not found")),
        }
    }
    let shape = match fluent_model {
        2 => all.as_slice().try_into().map(Shape::Tet).ok(),
        4 => all.as_slice().try_into().map(Shape::Hex).ok(),
        5 => all.as_slice().try_into().map(Shape::Pyramid).ok(),
        _ => all.as_slice().try_into().map(Shape::Prism).ok(),
    };
    shape.ok_or_else(|| MeshError::Invalid(format!("cell {cell}: the wrong number of vertices")))
}

/// `extrudedTriangleCellShape` and `extrudedQuadCellShape`: a prism or a
/// hexahedron from the cell's edges, with its front and back faces.
fn extrude(cell: usize, local: &[Vec<usize>], offset: usize) -> Result<(Shape, [Vec<usize>; 2])> {
    let edge = |k: usize| -> Result<(usize, usize)> {
        match local.get(k).map(Vec::as_slice) {
            Some(&[a, b]) => Ok((a, b)),
            _ => invalid(format!(
                "cell {cell}: a face of a 2-D cell has not two vertices"
            )),
        }
    };
    let up = |v: usize| v.saturating_add(offset);
    if let [_, _, _] = local {
        let (a0, a1) = edge(0)?;
        let other = if edge(1)?.0 == a1 {
            1
        } else if edge(2)?.0 == a1 {
            2
        } else {
            return invalid(format!("cell {cell}: problem with edge matching"));
        };
        let (_, c) = edge(other)?;
        let front = vec![a0, c, a1];
        let back = vec![up(a0), up(a1), up(c)];
        let shape = Shape::Prism([a0, a1, c, up(a0), up(a1), up(c)]);
        return Ok((shape, [front, back]));
    }
    let (a0, a1) = edge(0)?;
    for opposite in 1..4 {
        let (b0, b1) = edge(opposite)?;
        if a0 != b1 && a1 != b0 {
            let front = vec![a0, b1, b0, a1];
            let back = vec![up(a0), up(a1), up(b0), up(b1)];
            let shape = Shape::Hex([a0, a1, b0, b1, up(a0), up(a1), up(b0), up(b1)]);
            return Ok((shape, [front, back]));
        }
    }
    invalid(format!("cell {cell}: problem with edge matching"))
}

/// The ways to ask for a conversion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Every coordinate is multiplied by this (`-scale`).
    pub scale: f64,
    /// The thickness a 2-D mesh is extruded to, in the file's units and before
    /// scaling (`-2D`); 2 % of the diagonal of its box when left out.
    pub thickness: Option<f64>,
    /// Write the Fluent cell zones as `cellZones` and the faces inside each as
    /// `faceZones` (`-writeZones`).
    pub write_zones: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: 1.0,
            thickness: None,
            write_zones: false,
        }
    }
}

/// Whether two faces have the same vertices in the same order, from any
/// start and in either direction (`face::operator==`).
fn same_face(a: &[usize], b: &[usize]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let n = a.len();
    let Some(start) = b.iter().position(|&v| Some(&v) == a.first()) else {
        return false;
    };
    let at = |i: usize, back: bool| -> Option<usize> {
        let k = if back {
            (start + n - i % n) % n
        } else {
            (start + i) % n
        };
        b.get(k).copied()
    };
    [false, true]
        .iter()
        .any(|&back| (1..n).all(|i| at(i, back) == a.get(i).copied()))
}

pub(crate) enum Kind {
    Patch,
    Wall,
    Symmetry,
    Internal,
}

fn kind_of(zone_type: &str) -> Result<Kind> {
    Ok(match zone_type {
        "pressure" | "pressure-inlet" | "inlet-vent" | "intake-fan" | "pressure-outlet"
        | "exhaust-fan" | "outlet-vent" | "pressure-far-field" | "velocity-inlet"
        | "mass-flow-inlet" | "outflow" => Kind::Patch,
        "wall" => Kind::Wall,
        "symmetry" | "axis" => Kind::Symmetry,
        "interior" | "interface" | "internal" | "solid" | "fan" | "radiator" | "porous-jump"
        | "" => Kind::Internal,
        other => return invalid(format!("fluent patch type {other} not recognised")),
    })
}

/// A Fluent `.msh` file (ASCII) as the mesh `fluentMeshToFoam` makes of it.
pub fn fluent_to_foam(text: &str, options: Options) -> Result<RawPolyMesh> {
    let mut f = read(text)?;
    if f.points.is_empty() || f.model.is_empty() || f.faces.is_empty() {
        return invalid("the file has no points, faces or cells: is it a Fluent mesh?");
    }
    if f.dimension != 2 && f.dimension != 3 {
        return invalid(format!(
            "the dimension of the grid is {}, not 2 or 3",
            f.dimension
        ));
    }
    let n_cells = f.model.len();

    // The faces of each cell: first those it owns, then those it neighbours.
    let mut cell_faces: Vec<Vec<usize>> = vec![Vec::new(); n_cells];
    for pass in 0..2 {
        let side = if pass == 0 { &f.owner } else { &f.neighbour };
        for (face, &cell) in side.iter().enumerate() {
            if cell > -1 {
                let list = usize::try_from(cell)
                    .ok()
                    .and_then(|c| cell_faces.get_mut(c))
                    .ok_or_else(|| {
                        MeshError::Invalid(format!(
                            "face {} has cell {}, not in the mesh",
                            face + 1,
                            cell + 1
                        ))
                    })?;
                list.push(face);
            }
        }
    }
    for (cell, (list, &m)) in cell_faces.iter().zip(&f.model).enumerate() {
        match faces_of_model(m) {
            Some(n) if n == list.len() => {}
            Some(n) => {
                return invalid(format!(
                    "cell {} of type {m} has {} faces, not {n}",
                    cell + 1,
                    list.len()
                ));
            }
            None => return invalid(format!("cell {} has the unrecognised type {m}", cell + 1)),
        }
    }

    let mut shapes: Vec<Shape> = Vec::with_capacity(n_cells);
    let mut front_and_back: Vec<Vec<usize>> = Vec::new();
    if f.dimension == 2 {
        // Extrude into z: the front plane at +half, the back at -half.
        let (mut lo, mut hi) = (
            Vec3::new(f64::MAX, f64::MAX, 0.0),
            Vec3::new(f64::MIN, f64::MIN, 0.0),
        );
        for p in &f.points {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), 0.0);
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), 0.0);
        }
        let thickness = options
            .thickness
            .unwrap_or_else(|| 0.02 * ((hi.x - lo.x).powi(2) + (hi.y - lo.y).powi(2)).sqrt());
        let half = thickness / 2.0;
        let offset = f.points.len();
        let mut extruded: Vec<Vec3> = f.points.iter().map(|p| Vec3::new(p.x, p.y, half)).collect();
        extruded.extend(f.points.iter().map(|p| Vec3::new(p.x, p.y, -half)));
        for (cell, (list, &m)) in cell_faces.iter().zip(&f.model).enumerate() {
            if m != 1 && m != 3 {
                return invalid(format!("cell {} has the type {m}, not a 2-D one", cell + 1));
            }
            let local = outward_faces(&f, cell, list)?;
            let (shape, [front, back]) = extrude(cell, &local, offset)?;
            shapes.push(shape);
            front_and_back.push(front);
            front_and_back.push(back);
        }
        for face in &mut f.faces {
            match face.as_slice() {
                &[a, b] => *face = vec![a, b, b + offset, a + offset],
                other => {
                    return invalid(format!("a 2-D face is defined with {} points", other.len()));
                }
            }
        }
        f.points = extruded;
    } else {
        for (cell, (list, &m)) in cell_faces.iter().zip(&f.model).enumerate() {
            let local = outward_faces(&f, cell, list)?;
            shapes.push(recognise(cell, &local, m)?);
        }
    }
    if let Some(v) = shapes
        .iter()
        .flat_map(|s| s.labels())
        .copied()
        .find(|&v| v >= f.points.len())
    {
        return invalid(format!(
            "a cell uses point {}, but there are {}",
            v + 1,
            f.points.len()
        ));
    }
    for face in &f.faces {
        if let Some(&v) = face.iter().find(|&&v| v >= f.points.len()) {
            return invalid(format!(
                "a face uses point {}, but there are {}",
                v + 1,
                f.points.len()
            ));
        }
    }

    // A boundary face is given pointing into the domain: turn it round.
    for (face, &owner) in f.faces.iter_mut().zip(&f.owner) {
        if owner == -1 {
            face.reverse();
        }
    }
    for p in &mut f.points {
        *p = Vec3::new(
            p.x * options.scale,
            p.y * options.scale,
            p.z * options.scale,
        );
    }

    assemble(&f, &shapes, front_and_back, options)
}

/// A group of boundary faces and what the zone it came from makes of it.
pub(crate) struct Group {
    pub name: String,
    pub kind: Kind,
    pub faces: Vec<Vec<usize>>,
}

/// The groups the file's zones make, and the cell zones it asks for.
fn assemble(
    f: &Fluent,
    shapes: &[Shape],
    front_and_back: Vec<Vec<usize>>,
    options: Options,
) -> Result<RawPolyMesh> {
    let mut groups: Vec<Group> = Vec::new();
    for &(zone, first, last) in &f.face_groups {
        let zone_type = f.zone_type.get(&zone).map(String::as_str).unwrap_or("");
        let name = f.zone_name.get(&zone).cloned().unwrap_or_default();
        let kind = kind_of(zone_type)?;
        let wanted = range(first, last, f.faces.len(), "faces")?;
        let mut faces = Vec::with_capacity(wanted.len());
        for face in wanted {
            match f.faces.get(face) {
                Some(v) if v.len() == 3 || v.len() == 4 => faces.push(v.clone()),
                Some(v) => {
                    return invalid(format!("unrecognised face shape with {} vertices", v.len()));
                }
                None => return invalid("a face does not exist"),
            }
        }
        groups.push(Group { name, kind, faces });
    }
    let mut zones: Vec<(String, Vec<usize>)> = Vec::new();
    if options.write_zones && f.cell_groups.len() > 1 {
        for &(zone, first, last) in &f.cell_groups {
            let name = f.zone_name.get(&zone).cloned().unwrap_or_default();
            let wanted = range(first, last, shapes.len(), "cells")?;
            zones.push((name, wanted.collect()));
        }
    }
    let front_and_back = (f.dimension == 2).then_some(front_and_back);
    join(f.points.clone(), shapes, groups, front_and_back, zones)
}

/// The mesh of the shapes, its boundary faces put on the patches the groups
/// name (an `empty` `frontAndBackPlanes` for the faces of an extruded 2-D
/// mesh), and the zones.
pub(crate) fn join(
    points: Vec<Vec3>,
    shapes: &[Shape],
    mut groups: Vec<Group>,
    front_and_back: Option<Vec<Vec<usize>>>,
    zones: Vec<(String, Vec<usize>)>,
) -> Result<RawPolyMesh> {
    let mut cells = Cells::new();
    for shape in shapes {
        cells.push(&shape.faces());
    }
    // The mesh without patches: every boundary face in one default patch.
    let built = build(&cells, &[])?;
    let n_faces = built.owner.len();
    let (default_start, default_size) = built.default_patch;

    let mut by_point: HashMap<usize, Vec<usize>> = HashMap::new();
    for face in 0..n_faces {
        for &p in face_points(&built.faces, face) {
            by_point.entry(p).or_default().push(face);
        }
    }
    // `findFace`: the first face that uses the face's first point and equals it.
    let find = |wanted: &[usize]| -> Result<usize> {
        wanted
            .first()
            .and_then(|p| by_point.get(p))
            .and_then(|list| {
                list.iter()
                    .copied()
                    .find(|&m| same_face(wanted, face_points(&built.faces, m)))
            })
            .ok_or_else(|| {
                MeshError::Invalid(format!(
                    "cannot find a single face in the mesh which uses vertices {wanted:?}"
                ))
            })
    };
    let is_internal = |face: usize| face < built.n_internal;

    // The patches, in the order of the groups: a patch is a group whose zone
    // has a boundary type, and an inside one is dropped.
    struct Patch {
        name: String,
        kind: &'static str,
    }
    let mut patches: Vec<Patch> = Vec::new();
    let mut patch_of: HashMap<usize, usize> = HashMap::new();
    let extruded = front_and_back.is_some();
    if let Some(faces) = front_and_back {
        groups.push(Group {
            name: "frontAndBackPlanes".into(),
            kind: Kind::Internal,
            faces,
        });
    }
    let last = groups.len();
    for (index, Group { name, kind, faces }) in groups.iter().enumerate() {
        let is_front_and_back = extruded && index + 1 == last;
        let mut mesh_faces = Vec::with_capacity(faces.len());
        for face in faces {
            mesh_faces.push(find(face)?);
        }
        let kind = if is_front_and_back {
            "empty"
        } else {
            match kind {
                Kind::Patch => "patch",
                Kind::Wall => "wall",
                Kind::Symmetry => "symmetry",
                Kind::Internal => continue,
            }
        };
        let Some(&first) = mesh_faces.first() else {
            return invalid(format!("the patch {name} has no faces"));
        };
        if is_internal(first) {
            continue;
        }
        let id = patches.len();
        for &face in &mesh_faces {
            if is_internal(face) {
                return invalid(format!(
                    "face {face} on new patch {name} is not an external face of the mesh"
                ));
            }
            if patch_of.insert(face, id).is_some() {
                return invalid(format!(
                    "face {face} on new patch {name} has already been marked for repatching"
                ));
            }
        }
        patches.push(Patch {
            name: name.clone(),
            kind,
        });
    }

    // The faces no zone claimed make `default_wall`; the faces of each patch
    // keep the order they had.
    let boundary = default_start..default_start + default_size;
    let mut order: Vec<usize> = (0..built.n_internal).collect();
    let mut raw_patches: Vec<RawPatch> = Vec::new();
    let mut add = |name: String, kind: &str, faces: Vec<usize>, order: &mut Vec<usize>| {
        let start = order.len();
        order.extend(faces);
        let groups = match kind {
            "wall" | "empty" | "symmetry" => vec![kind.to_owned()],
            _ => Vec::new(),
        };
        raw_patches.push(RawPatch {
            name,
            kind: kind.to_owned(),
            start_face: start,
            n_faces: order.len() - start,
            groups,
            dict: Dictionary::new(),
        });
    };
    for (id, patch) in patches.iter().enumerate() {
        let faces = boundary
            .clone()
            .filter(|face| patch_of.get(face) == Some(&id))
            .collect();
        add(patch.name.clone(), patch.kind, faces, &mut order);
    }
    let leftover: Vec<usize> = boundary
        .filter(|face| !patch_of.contains_key(face))
        .collect();
    if !leftover.is_empty() {
        add("default_wall".into(), "wall", leftover, &mut order);
    }

    let mut faces = FaceList {
        offsets: vec![0],
        points: Vec::new(),
    };
    let mut owner = Vec::with_capacity(n_faces);
    for &face in &order {
        faces
            .points
            .extend_from_slice(face_points(&built.faces, face));
        faces.offsets.push(faces.points.len());
        owner.push(built.owner.get(face).copied().unwrap_or(0));
    }

    let mut cell_zones = Vec::new();
    let mut face_zones = Vec::new();
    for (name, wanted) in zones {
        let mut inside = vec![false; shapes.len()];
        for &cell in &wanted {
            if let Some(slot) = inside.get_mut(cell) {
                *slot = true;
            }
        }
        let faces_inside: Vec<usize> = (0..built.n_internal)
            .filter(|&face| {
                let own = built.owner.get(face).copied().unwrap_or(usize::MAX);
                let nei = built.neighbour.get(face).copied().unwrap_or(usize::MAX);
                inside.get(own).copied().unwrap_or(false)
                    && inside.get(nei).copied().unwrap_or(false)
            })
            .collect();
        face_zones.push(RawFaceZone {
            name: name.clone(),
            flip: vec![false; faces_inside.len()],
            faces: faces_inside,
        });
        cell_zones.push(RawCellZone {
            name,
            cells: wanted,
        });
    }

    Ok(RawPolyMesh {
        points,
        faces,
        owner,
        neighbour: built.neighbour,
        patches: raw_patches,
        cell_zones,
        face_zones,
    })
}
