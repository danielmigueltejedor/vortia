//! A CGNS mesh (the ADF file format) as the files of a `polyMesh`.
//!
//! CGNS is what ANSYS Fluent, ANSYS Meshing and many other tools export, and
//! its ADF flavour is a tree of nodes in one file: each node has a name, a
//! label, children and, perhaps, an array. This module reads that tree (both
//! byte orders; ASCII-hex disk pointers, the layout of the `A02011` files),
//! takes the single unstructured zone of the first base, and builds the mesh
//! the way the Gmsh and Fluent importers do:
//!
//! - the elements of the cell dimension (triangles and quadrilaterals for a
//!   2-D base, tetrahedra, pyramids, prisms and hexahedra for a 3-D one) are
//!   the cells, in the order of the file, and a cell that is inside out is
//!   turned round;
//! - the elements one dimension lower (edges, or triangles and
//!   quadrilaterals) are the boundary, one patch for each `Elements_t`
//!   section, named after it, a `wall` or a `symmetry` when the zone's
//!   `ZoneBC` says so by the same name;
//! - a 2-D mesh lies in a plane of the axes: it is extruded across that axis
//!   into prisms and hexahedra, with an `empty` `frontAndBackPlanes` patch,
//!   as `fluentMeshToFoam` does for a 2-D Fluent mesh;
//! - `-writeZones` writes each section of cells as a `cellZone`.
//!
//! Higher-order elements, polyhedral (`NGON_n`) meshes, several zones, links
//! to other files and data split in chunks are errors that say so. The
//! discrete data (`PID` and the like) and the solution are not read.

use vortia_core::Vec3;
use vortia_foamfile::RawPolyMesh;

use crate::blockmesh::shapes::Shape;
use crate::error::{MeshError, Result};
use crate::fluent::{Group, Kind, Options, join};
use crate::gmsh::points_outward;

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

/// Bytes of a node's fixed header, from its `NoDe` tag to its `TaiL`.
const NODE_TAIL: usize = 242;
/// One entry of a sub-node table: a 32 character name and a disk pointer.
const ENTRY: usize = 44;

/// An ADF file: its bytes and their order.
struct Adf<'a> {
    bytes: &'a [u8],
    little: bool,
}

/// What a node's header says; its array is read apart.
struct Node {
    name: String,
    label: String,
    n_sub: usize,
    n_entries: usize,
    sub_table: usize,
    data_type: String,
    dims: Vec<usize>,
    n_chunks: usize,
    chunk: usize,
}

impl<'a> Adf<'a> {
    fn open(bytes: &'a [u8]) -> Result<Self> {
        let head = bytes.get(..512.min(bytes.len())).unwrap_or_default();
        let find = |tag: &[u8]| head.windows(tag.len()).position(|w| w == tag);
        if bytes.get(4..24) != Some(b"ADF Database Version".as_slice()) {
            return invalid("not an ADF file (CGNS written as HDF5 is not supported yet)");
        }
        let little = match find(b"AdF2").and_then(|i| head.get(i + 4)) {
            Some(b'L') => true,
            Some(b'B') => false,
            _ => return invalid("this ADF file is neither big nor little endian IEEE"),
        };
        Ok(Self { bytes, little })
    }

    fn root(&self) -> Result<Node> {
        let head = self
            .bytes
            .get(..512.min(self.bytes.len()))
            .unwrap_or_default();
        let at = head
            .windows(4)
            .position(|w| w == b"AdF4")
            .ok_or_else(|| MeshError::Invalid("the ADF header has no root node".into()))?;
        self.node(self.pointer(at + 4)?)
    }

    fn slice(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        at.checked_add(len)
            .and_then(|end| self.bytes.get(at..end))
            .ok_or_else(|| MeshError::Invalid("the ADF file ends inside a node".into()))
    }

    fn hex(&self, at: usize, len: usize) -> Result<usize> {
        let text = std::str::from_utf8(self.slice(at, len)?).unwrap_or("");
        usize::from_str_radix(text, 16)
            .map_err(|_| MeshError::Invalid("a number of the ADF file is not hexadecimal".into()))
    }

    /// A disk pointer: eight hex digits of block (4096 bytes) and four of offset.
    fn pointer(&self, at: usize) -> Result<usize> {
        self.hex(at, 8)?
            .checked_mul(4096)
            .and_then(|b| b.checked_add(self.hex(at.saturating_add(8), 4).ok()?))
            .ok_or_else(|| MeshError::Invalid("a disk pointer of the ADF file is too large".into()))
    }

    fn text(&self, at: usize, len: usize) -> Result<String> {
        let raw = self.slice(at, len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(raw.get(..end).unwrap_or_default())
            .trim()
            .to_owned())
    }

    fn node(&self, at: usize) -> Result<Node> {
        if self.slice(at, 4)? != b"NoDe" {
            return invalid("a pointer of the ADF file does not lead to a node");
        }
        self.slice(at, NODE_TAIL + 4)?;
        let n_dims = self.hex(at + 128, 2)?;
        if n_dims > 12 {
            return invalid("a node has more than 12 dimensions");
        }
        let dims = (0..n_dims)
            .map(|i| self.hex(at + 130 + 8 * i, 8))
            .collect::<Result<Vec<_>>>()?;
        Ok(Node {
            name: self.text(at + 4, 32)?,
            label: self.text(at + 36, 32)?,
            n_sub: self.hex(at + 68, 8)?,
            n_entries: self.hex(at + 76, 8)?,
            sub_table: self.pointer(at + 84)?,
            data_type: self.text(at + 96, 32)?,
            dims,
            n_chunks: self.hex(at + 226, 4)?,
            chunk: self.pointer(at + 230)?,
        })
    }

    /// The children of a node, by their header.
    fn children(&self, node: &Node) -> Result<Vec<Node>> {
        if node.n_sub == 0 {
            return Ok(Vec::new());
        }
        if self.slice(node.sub_table, 4)? != b"SNTb" {
            return invalid("a node's table of children is missing");
        }
        if node.n_sub > node.n_entries {
            return invalid("a node has more children than its table holds");
        }
        let mut out = Vec::new();
        for i in 0..node.n_sub {
            let entry = node.sub_table.saturating_add(16 + i.saturating_mul(ENTRY));
            out.push(self.node(self.pointer(entry.saturating_add(32))?)?);
        }
        Ok(out)
    }

    /// The bytes of a node's array and how many items it holds.
    fn array(&self, node: &Node, item: usize) -> Result<(&'a [u8], usize)> {
        if node.dims.is_empty() || node.data_type == "MT" {
            return Ok((&[], 0));
        }
        if node.data_type == "LK" {
            return invalid(format!("{} is a link to another file", node.name));
        }
        if node.n_chunks != 1 {
            return invalid(format!("the array {} is stored in chunks", node.name));
        }
        let count = node
            .dims
            .iter()
            .try_fold(1_usize, |n, &d| n.checked_mul(d))
            .ok_or_else(|| MeshError::Invalid("an array is too large".into()))?;
        if self.slice(node.chunk, 4)? != b"DaTa" {
            return invalid(format!("the array {} has no data", node.name));
        }
        let len = count
            .checked_mul(item)
            .ok_or_else(|| MeshError::Invalid("an array is too large".into()))?;
        Ok((self.slice(node.chunk.saturating_add(16), len)?, count))
    }

    fn ints(&self, node: &Node) -> Result<Vec<i64>> {
        let item = match node.data_type.as_str() {
            "I4" | "U4" => 4,
            "I8" | "U8" => 8,
            "MT" => return Ok(Vec::new()),
            other => return invalid(format!("{} holds {other}, not integers", node.name)),
        };
        let (raw, _) = self.array(node, item)?;
        let mut out = Vec::with_capacity(raw.len() / item);
        for chunk in raw.chunks_exact(item) {
            let value = if item == 4 {
                let b: [u8; 4] = chunk.try_into().unwrap_or_default();
                i64::from(if self.little {
                    i32::from_le_bytes(b)
                } else {
                    i32::from_be_bytes(b)
                })
            } else {
                let b: [u8; 8] = chunk.try_into().unwrap_or_default();
                if self.little {
                    i64::from_le_bytes(b)
                } else {
                    i64::from_be_bytes(b)
                }
            };
            out.push(value);
        }
        Ok(out)
    }

    fn reals(&self, node: &Node) -> Result<Vec<f64>> {
        let item = match node.data_type.as_str() {
            "R4" => 4,
            "R8" => 8,
            other => return invalid(format!("{} holds {other}, not reals", node.name)),
        };
        let (raw, _) = self.array(node, item)?;
        let mut out = Vec::with_capacity(raw.len() / item);
        for chunk in raw.chunks_exact(item) {
            let value = if item == 4 {
                let b: [u8; 4] = chunk.try_into().unwrap_or_default();
                f64::from(if self.little {
                    f32::from_le_bytes(b)
                } else {
                    f32::from_be_bytes(b)
                })
            } else {
                let b: [u8; 8] = chunk.try_into().unwrap_or_default();
                if self.little {
                    f64::from_le_bytes(b)
                } else {
                    f64::from_be_bytes(b)
                }
            };
            out.push(value);
        }
        Ok(out)
    }

    fn string(&self, node: &Node) -> Result<String> {
        let (raw, _) = self.array(node, 1)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(raw.get(..end).unwrap_or_default())
            .trim()
            .to_owned())
    }
}

/// The CGNS element types the importer uses, by their number.
const BAR_2: i64 = 3;
const TRI_3: i64 = 5;
const QUAD_4: i64 = 7;
const TETRA_4: i64 = 10;
const PYRA_5: i64 = 12;
const PENTA_6: i64 = 14;
const HEXA_8: i64 = 17;
const MIXED: i64 = 20;

/// The dimension of an element type and how many nodes it has, or what is
/// wrong with it.
fn element(kind: i64) -> Result<(usize, usize)> {
    Ok(match kind {
        1 | 2 => (0, 1),
        BAR_2 => (1, 2),
        TRI_3 => (2, 3),
        QUAD_4 => (2, 4),
        TETRA_4 => (3, 4),
        PYRA_5 => (3, 5),
        PENTA_6 => (3, 6),
        HEXA_8 => (3, 8),
        4 | 6 | 8 | 9 | 11 | 13 | 15 | 16 | 18 | 19 | 21 | 24..=29 => {
            return invalid(format!(
                "element type {kind} is a higher-order element, which is not supported"
            ));
        }
        22 | 23 => {
            return invalid("polyhedral (NGON_n, NFACE_n) CGNS meshes are not supported yet");
        }
        other => return invalid(format!("element type {other} is not supported")),
    })
}

/// A section of elements, cut to those the mesh uses.
struct Section {
    name: String,
    /// Elements of the cell dimension: type and 0-based nodes.
    cells: Vec<(i64, Vec<usize>)>,
    /// Elements one dimension lower.
    faces: Vec<Vec<usize>>,
}

fn read_section(adf: &Adf<'_>, node: &Node, cell_dim: usize, n_points: usize) -> Result<Section> {
    let kind = adf.ints(node)?.first().copied().unwrap_or(0);
    let connectivity = adf
        .children(node)?
        .into_iter()
        .find(|c| c.name == "ElementConnectivity")
        .ok_or_else(|| MeshError::Invalid(format!("section {} has no connectivity", node.name)))?;
    let list = adf.ints(&connectivity)?;
    let mut section = Section {
        name: node.name.clone(),
        cells: Vec::new(),
        faces: Vec::new(),
    };
    let mut at = 0;
    while at < list.len() {
        let t = if kind == MIXED {
            let t = list.get(at).copied().unwrap_or(0);
            at += 1;
            t
        } else {
            kind
        };
        let (dim, count) = element(t)?;
        let nodes = list
            .get(at..at.saturating_add(count))
            .filter(|n| n.len() == count)
            .ok_or_else(|| {
                MeshError::Invalid(format!("section {} ends inside an element", node.name))
            })?;
        at += count;
        let mut zero = Vec::with_capacity(count);
        for &id in nodes {
            let index = usize::try_from(id)
                .ok()
                .and_then(|i| i.checked_sub(1))
                .filter(|&i| i < n_points)
                .ok_or_else(|| {
                    MeshError::Invalid(format!(
                        "section {} uses node {id}, but there are {n_points}",
                        node.name
                    ))
                })?;
            zero.push(index);
        }
        if dim == cell_dim {
            section.cells.push((t, zero));
        } else if dim + 1 == cell_dim {
            section.faces.push(zero);
        }
    }
    Ok(section)
}

/// A cell of three dimensions, turned round when it is inside out.
fn solid(kind: i64, v: &[usize], points: &[Vec3]) -> Result<Shape> {
    let bad = || MeshError::Invalid("an element has the wrong number of nodes".into());
    let at = |s: &Shape, order: &[usize]| -> Vec<usize> {
        let l = s.labels();
        order.iter().filter_map(|&i| l.get(i).copied()).collect()
    };
    let (shape, flipped): (Shape, Shape) = match kind {
        TETRA_4 => {
            let s = Shape::Tet(v.try_into().map_err(|_| bad())?);
            let f = at(&s, &[0, 2, 1, 3]).try_into().map_err(|_| bad())?;
            (s, Shape::Tet(f))
        }
        PYRA_5 => {
            let s = Shape::Pyramid(v.try_into().map_err(|_| bad())?);
            let f = at(&s, &[0, 3, 2, 1, 4]).try_into().map_err(|_| bad())?;
            (s, Shape::Pyramid(f))
        }
        PENTA_6 => {
            let s = Shape::Prism(v.try_into().map_err(|_| bad())?);
            let f = at(&s, &[0, 2, 1, 3, 5, 4]).try_into().map_err(|_| bad())?;
            (s, Shape::Prism(f))
        }
        _ => {
            let s = Shape::Hex(v.try_into().map_err(|_| bad())?);
            let f = at(&s, &[4, 5, 6, 7, 0, 1, 2, 3])
                .try_into()
                .map_err(|_| bad())?;
            (s, Shape::Hex(f))
        }
    };
    Ok(if points_outward(points, &shape) {
        shape
    } else {
        flipped
    })
}

/// The kind of patch a `BCType` makes.
fn kind_of(bc_type: &str) -> Kind {
    match bc_type {
        "BCWall"
        | "BCWallInviscid"
        | "BCWallViscous"
        | "BCWallViscousHeatFlux"
        | "BCWallViscousIsothermal" => Kind::Wall,
        "BCSymmetryPlane" | "BCSymmetryPolar" => Kind::Symmetry,
        _ => Kind::Patch,
    }
}

/// A CGNS file (ADF) as the mesh that `fluentMeshToFoam` or `gmshToFoam`
/// would make of the same elements.
pub fn cgns_to_foam(bytes: &[u8], options: Options) -> Result<RawPolyMesh> {
    let adf = Adf::open(bytes)?;
    let root = adf.root()?;
    let bases: Vec<Node> = adf
        .children(&root)?
        .into_iter()
        .filter(|c| c.label == "CGNSBase_t")
        .collect();
    let [base] = bases.as_slice() else {
        return invalid(format!("the file has {} bases, not one", bases.len()));
    };
    let dims = adf.ints(base)?;
    let cell_dim = match dims.first() {
        Some(&2) => 2,
        Some(&3) => 3,
        other => return invalid(format!("the cell dimension of the base is {other:?}")),
    };
    let zones: Vec<Node> = adf
        .children(base)?
        .into_iter()
        .filter(|c| c.label == "Zone_t")
        .collect();
    let [zone] = zones.as_slice() else {
        return invalid(format!("the base has {} zones, not one", zones.len()));
    };
    let parts = adf.children(zone)?;
    let part = |label: &str| parts.iter().find(|c| c.label == label);
    match part("ZoneType_t") {
        Some(t) if adf.string(t)? == "Unstructured" => {}
        _ => return invalid("the zone is not unstructured"),
    }
    let n_points = adf
        .ints(zone)?
        .first()
        .copied()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| MeshError::Invalid("the zone has no size".into()))?;

    // The coordinates; a missing one is zero.
    let grid = part("GridCoordinates_t")
        .ok_or_else(|| MeshError::Invalid("the zone has no coordinates".into()))?;
    let mut axes: [Option<Vec<f64>>; 3] = [None, None, None];
    for c in adf.children(grid)? {
        let index = match c.name.as_str() {
            "CoordinateX" => 0,
            "CoordinateY" => 1,
            "CoordinateZ" => 2,
            _ => continue,
        };
        let values = adf.reals(&c)?;
        if values.len() != n_points {
            return invalid(format!(
                "{} has {} values for {n_points} points",
                c.name,
                values.len()
            ));
        }
        if let Some(slot) = axes.get_mut(index) {
            *slot = Some(values);
        }
    }
    if axes.iter().flatten().count() < 2 {
        return invalid("the zone has fewer than two coordinates");
    }
    let coordinate = |axis: usize, i: usize| -> f64 {
        axes.get(axis)
            .and_then(Option::as_ref)
            .and_then(|v| v.get(i))
            .copied()
            .unwrap_or(0.0)
    };
    let mut points: Vec<Vec3> = (0..n_points)
        .map(|i| Vec3::new(coordinate(0, i), coordinate(1, i), coordinate(2, i)))
        .collect();
    if points
        .iter()
        .any(|p| !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()))
    {
        return invalid("a coordinate is not a finite number");
    }

    let mut sections = Vec::new();
    for node in parts.iter().filter(|c| c.label == "Elements_t") {
        sections.push(read_section(&adf, node, cell_dim, n_points)?);
    }

    // The kinds of patch the zone's boundary conditions give, by name.
    let mut kinds: Vec<(String, Kind)> = Vec::new();
    if let Some(bc) = part("ZoneBC_t") {
        for c in adf.children(bc)?.iter().filter(|c| c.label == "BC_t") {
            kinds.push((c.name.to_lowercase(), kind_of(&adf.string(c)?)));
        }
    }
    let kind_for = |name: &str| -> Kind {
        let wanted = name.to_lowercase();
        kinds
            .iter()
            .find(|(n, _)| *n == wanted)
            .map_or(Kind::Patch, |(_, k)| match k {
                Kind::Wall => Kind::Wall,
                Kind::Symmetry => Kind::Symmetry,
                _ => Kind::Patch,
            })
    };

    let n_cells: usize = sections.iter().map(|s| s.cells.len()).sum();
    if n_cells == 0 {
        return invalid("the file has no cells of the dimension of its base");
    }

    let mut shapes: Vec<Shape> = Vec::with_capacity(n_cells);
    let mut groups: Vec<Group> = Vec::new();
    let mut front_and_back = None;
    if cell_dim == 3 {
        for section in &sections {
            for (kind, nodes) in &section.cells {
                shapes.push(solid(*kind, nodes, &points)?);
            }
            if let Some(group) = boundary(section, &kind_for, |f| f.to_vec())? {
                groups.push(group);
            }
        }
    } else {
        // Extrude across the axis the mesh is flat in.
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in &points {
            for (axis, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                if let (Some(l), Some(h)) = (lo.get_mut(axis), hi.get_mut(axis)) {
                    *l = l.min(v);
                    *h = h.max(v);
                }
            }
        }
        let extent = |axis: usize| {
            hi.get(axis).copied().unwrap_or(0.0) - lo.get(axis).copied().unwrap_or(0.0)
        };
        let normal = (0..3)
            .min_by(|&a, &b| extent(a).total_cmp(&extent(b)))
            .unwrap_or(2);
        let (u, v) = ((normal + 1) % 3, (normal + 2) % 3);
        let flat = extent(normal);
        let wide = extent(u).max(extent(v));
        if wide <= 0.0 || flat > 1e-6 * wide {
            return invalid("a 2-D CGNS mesh must lie in a plane of the axes");
        }
        let thickness = options
            .thickness
            .unwrap_or_else(|| 0.02 * (extent(u).powi(2) + extent(v).powi(2)).sqrt());
        let half = thickness / 2.0;
        let offset = points.len();
        let axis_of = |p: &Vec3, axis: usize| match axis {
            0 => p.x,
            1 => p.y,
            _ => p.z,
        };
        let moved = |p: &Vec3, by: f64| match normal {
            0 => Vec3::new(p.x + by, p.y, p.z),
            1 => Vec3::new(p.x, p.y + by, p.z),
            _ => Vec3::new(p.x, p.y, p.z + by),
        };
        let mut extruded: Vec<Vec3> = points.iter().map(|p| moved(p, -half)).collect();
        extruded.extend(points.iter().map(|p| moved(p, half)));
        let mut planes: Vec<Vec<usize>> = Vec::new();
        for section in &sections {
            for (kind, nodes) in &section.cells {
                let mut ring = nodes.clone();
                let area: f64 = ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .map(|(&a, &b)| {
                        let (pa, pb) = (points.get(a), points.get(b));
                        match (pa, pb) {
                            (Some(pa), Some(pb)) => {
                                axis_of(pa, u) * axis_of(pb, v) - axis_of(pb, u) * axis_of(pa, v)
                            }
                            _ => 0.0,
                        }
                    })
                    .sum();
                if area == 0.0 {
                    return invalid("a 2-D cell has no area");
                }
                if area < 0.0 {
                    ring.reverse();
                }
                let up = |v: &usize| v + offset;
                let top: Vec<usize> = ring.iter().map(up).collect();
                let shape = match (*kind, ring.as_slice()) {
                    (TRI_3, &[a, b, c]) => {
                        Shape::Prism([a, b, c, a + offset, b + offset, c + offset])
                    }
                    (_, &[a, b, c, d]) => {
                        Shape::Hex([a, b, c, d, a + offset, b + offset, c + offset, d + offset])
                    }
                    _ => return invalid("an element has the wrong number of nodes"),
                };
                shapes.push(shape);
                planes.push(ring.iter().rev().copied().collect());
                planes.push(top);
            }
            let edge = |f: &[usize]| match f {
                &[a, b] => vec![a, b, b + offset, a + offset],
                other => other.to_vec(),
            };
            if let Some(group) = boundary(section, &kind_for, edge)? {
                groups.push(group);
            }
        }
        points = extruded;
        front_and_back = Some(planes);
    }
    for p in &mut points {
        *p = Vec3::new(
            p.x * options.scale,
            p.y * options.scale,
            p.z * options.scale,
        );
    }

    let mut zones = Vec::new();
    if options.write_zones {
        let mut first = 0;
        let with_cells = sections.iter().filter(|s| !s.cells.is_empty()).count();
        for section in &sections {
            let end = first + section.cells.len();
            if with_cells > 1 && !section.cells.is_empty() {
                zones.push((section.name.clone(), (first..end).collect()));
            }
            first = end;
        }
    }
    join(points, &shapes, groups, front_and_back, zones)
}

/// The patch a section's boundary elements make, with each face as the
/// mesh holds it.
fn boundary(
    section: &Section,
    kind_for: &dyn Fn(&str) -> Kind,
    face: impl Fn(&[usize]) -> Vec<usize>,
) -> Result<Option<Group>> {
    if section.faces.is_empty() {
        return Ok(None);
    }
    Ok(Some(Group {
        name: section.name.clone(),
        kind: kind_for(&section.name),
        faces: section.faces.iter().map(|f| face(f)).collect(),
    }))
}
