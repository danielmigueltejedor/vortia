//! The engine's own `gmshToFoam`: a Gmsh `.msh` file (format 1, 2 or 4, ASCII)
//! as the files of a `polyMesh`.
//!
//! Derived from OpenFOAM's `gmshToFoam` (GPL-3.0-or-later, like Vortia), so
//! that the same file gives the same mesh, point for point and face for face,
//! quirks included: the sections are read line by line as it reads them, the
//! physical regions of the surface elements become patches (named after the
//! physical names) and those of the volume elements cell zones, prisms and
//! hexahedra that Gmsh wrote inside out are turned round, the faces of the
//! patches are found among the boundary faces of the cells by their
//! vertices, and a surface element that matches an inside face makes a face
//! zone of its region.

use std::collections::HashMap;

use vortia_core::Vec3;
use vortia_foamfile::{
    Dictionary, Entry, FaceList, RawCellZone, RawFaceZone, RawPatch, RawPolyMesh, Value,
};

use crate::blockmesh::shapes::{Cells, Shape, build};
use crate::blockmesh::{cell_centre, face_area_normal};
use crate::error::{MeshError, Result};

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

// Gmsh's element type numbers.
const LINE: i64 = 1;
const TRI: i64 = 2;
const QUAD: i64 = 3;
const TET: i64 = 4;
const HEX: i64 = 5;
const PRISM: i64 = 6;
const PYR: i64 = 7;

/// `word::validate`: a physical name as a word, without the characters a
/// word cannot hold.
fn validate(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '/' | ';' | '{' | '}'))
        .collect()
}

/// The lines of the file, handed out as `IFstream::getLine` does: an empty
/// line once the file is over.
struct Lines<'a> {
    all: Vec<&'a str>,
    at: usize,
}

impl<'a> Lines<'a> {
    fn next(&mut self) -> &'a str {
        let line = self.all.get(self.at).copied().unwrap_or("");
        self.at += 1;
        line
    }

    /// The number of the line read last, counted from one.
    fn number(&self) -> usize {
        self.at
    }

    fn at_end(&self) -> bool {
        self.at >= self.all.len()
    }
}

/// The whitespace-separated fields of a line, read as numbers.
struct Fields<'a> {
    it: std::str::SplitWhitespace<'a>,
    line: usize,
}

impl<'a> Fields<'a> {
    fn new(text: &'a str, line: usize) -> Self {
        Self {
            it: text.split_whitespace(),
            line,
        }
    }

    fn word(&mut self) -> Option<&'a str> {
        self.it.next()
    }

    fn label(&mut self) -> Result<i64> {
        let Some(t) = self.it.next() else {
            return invalid(format!("line {}: a number is missing", self.line));
        };
        t.parse::<i64>().or_else(|_| {
            invalid(format!(
                "line {}: expected a whole number, found {t}",
                self.line
            ))
        })
    }

    fn scalar(&mut self) -> Result<f64> {
        let Some(t) = self.it.next() else {
            return invalid(format!("line {}: a number is missing", self.line));
        };
        t.parse::<f64>().ok().filter(|x| x.is_finite()).map_or_else(
            || invalid(format!("line {}: expected a number, found {t}", self.line)),
            Ok,
        )
    }
}

/// What the file has been read into.
#[derive(Default)]
struct Msh {
    points: Vec<Vec3>,
    /// Gmsh's node numbers to ours, in the order the nodes were read.
    msh_to_foam: HashMap<i64, usize>,
    cells: Vec<Shape>,
    patch_to_phys: Vec<i64>,
    patch_faces: Vec<Vec<Vec<usize>>>,
    phys_to_patch: HashMap<i64, usize>,
    zone_to_phys: Vec<i64>,
    zone_cells: Vec<Vec<usize>>,
    phys_to_zone: HashMap<i64, usize>,
    physical_names: HashMap<i64, String>,
    surface_entity_to_phys: HashMap<i64, i64>,
    volume_entity_to_phys: HashMap<i64, i64>,
}

impl Msh {
    fn renumber(&self, nodes: &[i64]) -> Result<Vec<usize>> {
        nodes
            .iter()
            .map(|n| {
                self.msh_to_foam.get(n).copied().map_or_else(
                    || invalid(format!("the element uses the node {n}, which was not read")),
                    Ok,
                )
            })
            .collect()
    }

    fn add_face(&mut self, phys: i64, face: Vec<usize>) {
        let patch = match self.phys_to_patch.get(&phys) {
            Some(&p) => p,
            None => {
                let p = self.patch_faces.len();
                self.patch_faces.push(Vec::new());
                self.patch_to_phys.push(phys);
                self.phys_to_patch.insert(phys, p);
                p
            }
        };
        if let Some(faces) = self.patch_faces.get_mut(patch) {
            faces.push(face);
        }
    }

    fn store_in_zone(&mut self, phys: i64, cell: usize) {
        let zone = match self.phys_to_zone.get(&phys) {
            Some(&z) => z,
            None => {
                let z = self.zone_cells.len();
                self.zone_cells.push(Vec::new());
                self.zone_to_phys.push(phys);
                self.phys_to_zone.insert(phys, z);
                z
            }
        };
        if let Some(cells) = self.zone_cells.get_mut(zone) {
            cells.push(cell);
        }
    }

    /// One element of the file: a surface element is a face of a patch, a
    /// volume element a cell (of the zone of its physical region).
    fn add_element(
        &mut self,
        kind: i64,
        phys: i64,
        fields: &mut Fields<'_>,
        keep_orientation: bool,
    ) -> Result<()> {
        let mut nodes = |n: usize| -> Result<Vec<i64>> { (0..n).map(|_| fields.label()).collect() };
        match kind {
            LINE => {
                nodes(2)?;
            }
            TRI | QUAD => {
                let face = self.renumber(&nodes(if kind == TRI { 3 } else { 4 })?)?;
                self.add_face(phys, face);
            }
            TET | PYR | PRISM | HEX => {
                let n = match kind {
                    TET => 4,
                    PYR => 5,
                    PRISM => 6,
                    _ => 8,
                };
                let cell = self.cells.len();
                self.store_in_zone(phys, cell);
                let v = self.renumber(&nodes(n)?)?;
                let shape = self.shape(kind, &v, keep_orientation)?;
                self.cells.push(shape);
            }
            // Anything else (points, higher-order elements) is not used.
            _ => {}
        }
        Ok(())
    }

    fn shape(&self, kind: i64, v: &[usize], keep_orientation: bool) -> Result<Shape> {
        let bad = || MeshError::Invalid("an element has the wrong number of nodes".into());
        match kind {
            TET => Ok(Shape::Tet(v.try_into().map_err(|_| bad())?)),
            PYR => Ok(Shape::Pyramid(v.try_into().map_err(|_| bad())?)),
            PRISM => {
                let shape = Shape::Prism(v.try_into().map_err(|_| bad())?);
                if keep_orientation || self.correct_orientation(&shape) {
                    return Ok(shape);
                }
                let c = shape.labels();
                let at = |i: usize| c.get(i).copied().unwrap_or_default();
                Ok(Shape::Prism([at(0), at(2), at(1), at(3), at(4), at(5)]))
            }
            _ => {
                let shape = Shape::Hex(v.try_into().map_err(|_| bad())?);
                if keep_orientation || self.correct_orientation(&shape) {
                    return Ok(shape);
                }
                let c = shape.labels();
                let at = |i: usize| c.get(i).copied().unwrap_or_default();
                Ok(Shape::Hex([
                    at(4),
                    at(5),
                    at(6),
                    at(7),
                    at(0),
                    at(1),
                    at(2),
                    at(3),
                ]))
            }
        }
    }

    fn point(&self, i: usize) -> Vec3 {
        self.points.get(i).copied().unwrap_or_default()
    }

    /// `correctOrientation`: no face of the cell points towards its centre.
    fn correct_orientation(&self, shape: &Shape) -> bool {
        let labels = shape.labels();
        let estimate = labels
            .iter()
            .fold(Vec3::default(), |s, &l| s + self.point(l))
            / labels.len().max(1) as f64;
        let faces: Vec<Vec<usize>> = shape.faces();
        let coords: Vec<Vec<Vec3>> = faces
            .iter()
            .map(|f| f.iter().map(|&l| self.point(l)).collect())
            .collect();
        let centre = cell_centre(&coords, estimate);
        faces.iter().zip(&coords).all(|(f, c)| {
            let first = f.first().map_or_else(Vec3::default, |&l| self.point(l));
            (first - centre).dot(face_area_normal(c)) >= 0.0
        })
    }
}

/// `$MeshFormat`: the version; only ASCII files can be read.
fn read_format(lines: &mut Lines<'_>) -> Result<f64> {
    let mut f = Fields::new(lines.next(), lines.number());
    let version = f.scalar()?;
    if f.label()? != 0 {
        return invalid("Can only read ascii msh files.");
    }
    f.label()?;
    let tag = lines.next().split_whitespace().next().unwrap_or("");
    if tag != "$EndMeshFormat" {
        return invalid(format!(
            "line {}: did not find the $EndMeshFormat tag",
            lines.number()
        ));
    }
    Ok(version)
}

fn end_tag(lines: &mut Lines<'_>, tags: &[&str]) -> Result<()> {
    let tag = lines.next().split_whitespace().next().unwrap_or("");
    if tags.contains(&tag) {
        Ok(())
    } else {
        invalid(format!(
            "line {}: did not find the {} tag, found `{tag}`",
            lines.number(),
            tags.join(" or ")
        ))
    }
}

/// `$NOD` of the formats before 4: the node count, then `number x y z`.
fn read_points_legacy(lines: &mut Lines<'_>, msh: &mut Msh) -> Result<()> {
    let n = Fields::new(lines.next(), lines.number()).label()?;
    let n = usize::try_from(n).or_else(|_| invalid("a negative number of nodes"))?;
    for i in 0..n {
        let mut f = Fields::new(lines.next(), lines.number());
        let label = f.label()?;
        let p = Vec3::new(f.scalar()?, f.scalar()?, f.scalar()?);
        msh.points.push(p);
        msh.msh_to_foam.entry(label).or_insert(i);
    }
    end_tag(lines, &["$ENDNOD", "$EndNodes"])
}

/// `$Nodes` of format 4: blocks of nodes per entity, each node's number on a
/// line of its own and then its coordinates. As in OpenFOAM, a node is stored
/// at its number minus one, and is called by the place it was read at.
fn read_points(lines: &mut Lines<'_>, msh: &mut Msh) -> Result<()> {
    let mut head = Fields::new(lines.next(), lines.number());
    let n_entities = head.label()?;
    let n = usize::try_from(head.label()?).or_else(|_| invalid("a negative number of nodes"))?;
    msh.points = Vec::new();
    msh.points.resize(n.min(1 << 24), Vec3::default());
    let mut read = 0usize;
    for _ in 0..n_entities.max(0) {
        let mut f = Fields::new(lines.next(), lines.number());
        f.label()?;
        f.label()?;
        f.label()?;
        let n_nodes =
            usize::try_from(f.label()?).or_else(|_| invalid("negative nodes in a block"))?;
        let mut ids = Vec::new();
        for _ in 0..n_nodes {
            ids.push(Fields::new(lines.next(), lines.number()).label()?);
        }
        for id in ids {
            let mut f = Fields::new(lines.next(), lines.number());
            let p = Vec3::new(f.scalar()?, f.scalar()?, f.scalar()?);
            let slot = usize::try_from(id - 1)
                .ok()
                .filter(|&i| i < n)
                .ok_or_else(|| MeshError::Invalid(format!("node {id} is out of range")))?;
            if slot >= msh.points.len() {
                msh.points.resize(slot + 1, Vec3::default());
            }
            if let Some(at) = msh.points.get_mut(slot) {
                *at = p;
            }
            msh.msh_to_foam.entry(id).or_insert(read);
            read += 1;
        }
    }
    end_tag(lines, &["$ENDNOD", "$EndNodes"])
}

/// `$PhysicalNames`: `[type] number "name"`; a name that is given twice
/// keeps its first meaning.
fn read_physical_names(lines: &mut Lines<'_>, msh: &mut Msh) -> Result<()> {
    let n = Fields::new(lines.next(), lines.number()).label()?;
    for _ in 0..n.max(0) {
        if lines.at_end() {
            return invalid("the file ends inside the physical names");
        }
        let text = lines.next();
        let mut f = Fields::new(text, lines.number());
        let spaces = text.matches(' ').count();
        let (region, name) = match spaces {
            1 => {
                let region = f.label()?;
                (region, f.word())
            }
            2 => {
                f.label()?;
                let region = f.label()?;
                (region, f.word())
            }
            _ => continue,
        };
        let name = name.unwrap_or("").trim_matches('"');
        msh.physical_names
            .entry(region)
            .or_insert_with(|| validate(name));
    }
    end_tag(lines, &["$EndPhysicalNames"])
}

/// `$Entities` of format 4: which physical region each surface and volume
/// entity is in.
fn read_entities(lines: &mut Lines<'_>, msh: &mut Msh) -> Result<()> {
    let mut head = Fields::new(lines.next(), lines.number());
    let (n_points, n_curves, n_surfaces, n_volumes) =
        (head.label()?, head.label()?, head.label()?, head.label()?);
    for _ in 0..n_points.max(0).saturating_add(n_curves.max(0)) {
        if lines.at_end() {
            return invalid("the file ends inside the entities");
        }
        lines.next();
    }
    for (count, volumes) in [(n_surfaces, false), (n_volumes, true)] {
        for _ in 0..count.max(0) {
            let mut f = Fields::new(lines.next(), lines.number());
            let entity = f.label()?;
            for _ in 0..6 {
                f.scalar()?;
            }
            if f.label()? > 1 {
                return invalid(format!(
                    "line {}: Cannot interpret multiple physical groups associated with one entity",
                    lines.number()
                ));
            }
            let phys = f.label()?;
            let map = if volumes {
                &mut msh.volume_entity_to_phys
            } else {
                &mut msh.surface_entity_to_phys
            };
            map.entry(entity).or_insert(phys);
        }
    }
    end_tag(lines, &["$EndEntities"])
}

/// `$ELM` of the formats before 4: `number type [tags] nodes`.
fn read_cells_legacy(
    lines: &mut Lines<'_>,
    msh: &mut Msh,
    version: f64,
    keep_orientation: bool,
) -> Result<()> {
    let n = Fields::new(lines.next(), lines.number()).label()?;
    for _ in 0..n.max(0) {
        let mut f = Fields::new(lines.next(), lines.number());
        let (kind, phys);
        if version >= 2.0 {
            f.label()?;
            kind = f.label()?;
            let n_tags = f.label()?;
            let mut first = 0;
            for t in 0..n_tags.max(0) {
                let tag = f.label()?;
                if t == 0 {
                    first = tag;
                }
            }
            phys = first;
        } else {
            f.label()?;
            kind = f.label()?;
            phys = f.label()?;
            f.label()?;
            f.label()?;
        }
        msh.add_element(kind, phys, &mut f, keep_orientation)?;
    }
    end_tag(lines, &["$ENDELM", "$EndElements"])?;
    Ok(())
}

/// `$Elements` of format 4: blocks of elements per entity.
fn read_cells(lines: &mut Lines<'_>, msh: &mut Msh, keep_orientation: bool) -> Result<()> {
    let n_entities = Fields::new(lines.next(), lines.number()).label()?;
    for _ in 0..n_entities.max(0) {
        let mut f = Fields::new(lines.next(), lines.number());
        let (dim, entity, kind, n) = (f.label()?, f.label()?, f.label()?, f.label()?);
        let phys = match dim {
            2 => msh.surface_entity_to_phys.get(&entity).copied(),
            3 => msh.volume_entity_to_phys.get(&entity).copied(),
            _ => Some(0),
        }
        .ok_or_else(|| {
            MeshError::Invalid(format!(
                "line {}: the entity {entity} has no physical group",
                lines.number()
            ))
        })?;
        for _ in 0..n.max(0) {
            let mut f = Fields::new(lines.next(), lines.number());
            f.label()?;
            msh.add_element(kind, phys, &mut f, keep_orientation)?;
        }
    }
    end_tag(lines, &["$ENDELM", "$EndElements"])
}

/// `skipSection`: to the line that starts with `$End`.
fn skip_section(lines: &mut Lines<'_>) -> bool {
    loop {
        let eof = lines.at_end();
        let line = lines.next();
        if eof {
            return false;
        }
        if line.len() >= 4 && line.starts_with("$End") {
            return true;
        }
    }
}

/// The mesh a Gmsh `.msh` file holds. `keep_orientation` leaves prisms and
/// hexahedra as Gmsh wrote them, instead of turning round those that are
/// inside out.
pub fn gmsh_to_foam(text: &str, keep_orientation: bool) -> Result<RawPolyMesh> {
    let mut lines = Lines {
        all: text.split('\n').collect(),
        at: 0,
    };
    let mut msh = Msh::default();
    let mut version = 1.0;
    loop {
        let line = lines.next();
        if line.is_empty() {
            break;
        }
        match line.split_whitespace().next().unwrap_or("") {
            "$MeshFormat" => version = read_format(&mut lines)?,
            "$PhysicalNames" => read_physical_names(&mut lines, &mut msh)?,
            "$Entities" => read_entities(&mut lines, &mut msh)?,
            "$NOD" | "$Nodes" => {
                if version < 4.0 {
                    read_points_legacy(&mut lines, &mut msh)?;
                } else {
                    read_points(&mut lines, &mut msh)?;
                }
            }
            "$ELM" | "$Elements" => {
                if version < 4.0 {
                    read_cells_legacy(&mut lines, &mut msh, version, keep_orientation)?;
                } else {
                    read_cells(&mut lines, &mut msh, keep_orientation)?;
                }
            }
            _ => {
                if !skip_section(&mut lines) {
                    break;
                }
            }
        }
    }
    if msh.cells.is_empty() {
        return invalid(
            "no cells were read: does the file hold any 3D elements (hex, prism, pyramid, tet)?",
        );
    }
    assemble(msh)
}

/// The mesh of the cells, its boundary faces put on the patches the surface
/// elements name, and the zones.
fn assemble(msh: Msh) -> Result<RawPolyMesh> {
    let mut cells = Cells::new();
    for shape in &msh.cells {
        cells.push(&shape.faces());
    }
    // All the boundary faces on one patch first, then the elements' faces
    // looked for among them, as `polyMesh` and `repatchPolyTopoChanger` do.
    let built = build(&cells, &[])?;
    let n_faces = built.owner.len();
    let (default_start, default_size) = built.default_patch;

    let mut on_boundary: HashMap<usize, Vec<usize>> = HashMap::new();
    for face in default_start..default_start + default_size {
        for &p in face_points(&built.faces, face) {
            on_boundary.entry(p).or_default().push(face);
        }
    }
    let mut every_face: Option<HashMap<usize, Vec<usize>>> = None;

    // `findFace` and `findInternalFace`: the first face that has all the
    // element's vertices (counting the face's own vertices that are among
    // them, so a quadrilateral also answers for a triangle it contains).
    let uses_all = |face: usize, wanted: &[usize]| {
        let n = face_points(&built.faces, face)
            .iter()
            .filter(|p| wanted.contains(p))
            .count();
        n == wanted.len()
    };

    let mut patch_of: HashMap<usize, usize> = HashMap::new();
    let mut zone_faces: Vec<Vec<usize>> = vec![Vec::new(); msh.patch_faces.len()];
    for (patch, faces) in msh.patch_faces.iter().enumerate() {
        for wanted in faces {
            let Some(&first) = wanted.first() else {
                continue;
            };
            let on_edge = on_boundary
                .get(&first)
                .and_then(|list| list.iter().copied().find(|&f| uses_all(f, wanted)));
            if let Some(face) = on_edge {
                patch_of.insert(face, patch);
                continue;
            }
            let all = every_face.get_or_insert_with(|| {
                let mut m: HashMap<usize, Vec<usize>> = HashMap::new();
                for face in 0..n_faces {
                    for &p in face_points(&built.faces, face) {
                        m.entry(p).or_default().push(face);
                    }
                }
                m
            });
            let inside = all
                .get(&first)
                .and_then(|list| list.iter().copied().find(|&f| uses_all(f, wanted)));
            if let (Some(face), Some(zone)) = (inside, zone_faces.get_mut(patch)) {
                zone.push(face);
            }
        }
    }

    // `polyTopoChange` puts the faces of each patch in the order they had.
    let name_of =
        |phys: i64, default: String| msh.physical_names.get(&phys).cloned().unwrap_or(default);
    let mut order: Vec<usize> = (0..built.n_internal).collect();
    let mut patches: Vec<RawPatch> = Vec::new();
    let patch_entry = |name: String, start: usize, size: usize, physical: bool| {
        let mut dict = Dictionary::new();
        if physical {
            dict.insert(
                "physicalType",
                Entry::Values(vec![Value::Word("patch".into())]),
            );
        }
        RawPatch {
            name,
            kind: "patch".into(),
            start_face: start,
            n_faces: size,
            groups: Vec::new(),
            dict,
        }
    };
    for patch in 0..msh.patch_faces.len() {
        let start = order.len();
        order.extend(
            (default_start..default_start + default_size)
                .filter(|f| patch_of.get(f) == Some(&patch)),
        );
        let phys = msh.patch_to_phys.get(patch).copied().unwrap_or(0);
        patches.push(patch_entry(
            name_of(phys, format!("patch{patch}")),
            start,
            order.len() - start,
            true,
        ));
    }
    let start = order.len();
    order.extend(
        (default_start..default_start + default_size).filter(|f| !patch_of.contains_key(f)),
    );
    if order.len() > start {
        patches.push(patch_entry(
            "defaultFaces".into(),
            start,
            order.len() - start,
            false,
        ));
    }

    let mut faces = FaceList {
        offsets: vec![0],
        points: Vec::new(),
    };
    let mut owner = Vec::with_capacity(n_faces);
    for &f in &order {
        faces.points.extend_from_slice(face_points(&built.faces, f));
        faces.offsets.push(faces.points.len());
        owner.push(built.owner.get(f).copied().unwrap_or(0));
    }

    let cell_zones = msh
        .zone_cells
        .iter()
        .enumerate()
        .filter(|(_, cells)| !cells.is_empty())
        .map(|(zone, cells)| RawCellZone {
            name: name_of(
                msh.zone_to_phys.get(zone).copied().unwrap_or(0),
                format!("cellZone_{zone}"),
            ),
            cells: cells.clone(),
        })
        .collect();
    let face_zones = zone_faces
        .iter()
        .enumerate()
        .filter(|(_, faces)| !faces.is_empty())
        .map(|(zone, faces)| RawFaceZone {
            name: name_of(
                msh.patch_to_phys.get(zone).copied().unwrap_or(0),
                format!("faceZone_{zone}"),
            ),
            faces: faces.clone(),
            flip: vec![true; faces.len()],
        })
        .collect();

    Ok(RawPolyMesh {
        points: msh.points,
        faces,
        owner,
        neighbour: built.neighbour,
        patches,
        cell_zones,
        face_zones,
    })
}

pub(crate) fn face_points(faces: &FaceList, face: usize) -> &[usize] {
    match (faces.offsets.get(face), faces.offsets.get(face + 1)) {
        (Some(&a), Some(&b)) => faces.points.get(a..b).unwrap_or_default(),
        _ => &[],
    }
}
