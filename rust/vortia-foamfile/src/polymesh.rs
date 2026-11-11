//! The five files of `constant/polyMesh`, read as they are on disk.
//!
//! Nothing here checks that the files agree with each other: that is the
//! mesh's job (`vortia-mesh`), which knows what a valid mesh is.

use std::path::Path;

use vortia_core::Vec3;

use std::fmt::Write as _;

use crate::dict::{Dictionary, Entry, Value};
use crate::error::{FoamError, Result};
use crate::file::FoamFile;
use crate::lists::{self, FaceList};
use crate::token::Tokenizer;
use crate::write;

/// One entry of the `boundary` file.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPatch {
    pub name: String,
    /// `patch`, `wall`, `empty`, `wedge`, `symmetryPlane`, `cyclic`, …
    pub kind: String,
    pub start_face: usize,
    pub n_faces: usize,
    pub groups: Vec<String>,
    /// The whole entry, for types with settings of their own (cyclic, processor).
    pub dict: Dictionary,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawPolyMesh {
    pub points: Vec<Vec3>,
    pub faces: FaceList,
    pub owner: Vec<usize>,
    pub neighbour: Vec<usize>,
    pub patches: Vec<RawPatch>,
}

impl RawPolyMesh {
    /// Reads `<case>/constant/polyMesh`.
    pub fn read_case(case: impl AsRef<Path>) -> Result<Self> {
        Self::read_dir(case.as_ref().join("constant").join("polyMesh"))
    }

    /// Reads a `polyMesh` directory.
    pub fn read_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        Ok(Self {
            points: read_with(&dir.join("points"), lists::read_vectors)?,
            faces: read_with(&dir.join("faces"), lists::read_faces)?,
            owner: read_with(&dir.join("owner"), lists::read_labels)?,
            neighbour: read_with(&dir.join("neighbour"), lists::read_labels)?,
            patches: read_with(&dir.join("boundary"), read_boundary)?,
        })
    }
}

impl RawPolyMesh {
    /// Writes the five files into `<case>/constant/polyMesh`.
    pub fn write_case(&self, case: impl AsRef<Path>) -> Result<()> {
        self.write_dir(case.as_ref().join("constant").join("polyMesh"))
    }

    /// Writes the five files into a `polyMesh` directory, each atomically.
    pub fn write_dir(&self, dir: impl AsRef<Path>) -> Result<()> {
        let dir = dir.as_ref();
        let file = |name: &str, class: &str, body: &str| {
            let mut out = String::new();
            write::header(&mut out, class, Some("constant/polyMesh"), name);
            out.push_str(body);
            write::footer(&mut out);
            write::atomic(&dir.join(name), out.as_bytes())
        };

        let mut body = format!("{}\n(\n", self.points.len());
        for p in &self.points {
            let _ = writeln!(
                body,
                "({} {} {})",
                write::scalar(p.x),
                write::scalar(p.y),
                write::scalar(p.z)
            );
        }
        body.push_str(")\n");
        file("points", "vectorField", &body)?;

        let mut body = format!("{}\n(\n", self.faces.len());
        for i in 0..self.faces.len() {
            let face = self.faces.face(i);
            let _ = write!(body, "{}(", face.len());
            for (k, p) in face.iter().enumerate() {
                if k > 0 {
                    body.push(' ');
                }
                let _ = write!(body, "{p}");
            }
            body.push_str(")\n");
        }
        body.push_str(")\n");
        file("faces", "faceList", &body)?;

        file("owner", "labelList", &labels(&self.owner))?;
        file("neighbour", "labelList", &labels(&self.neighbour))?;

        let mut body = format!("{}\n(\n", self.patches.len());
        for p in &self.patches {
            let mut d = Dictionary::new();
            d.insert("type", Entry::Values(vec![Value::Word(p.kind.clone())]));
            if !p.groups.is_empty() {
                let groups = p.groups.iter().map(|g| Value::Word(g.clone())).collect();
                d.insert("inGroups", Entry::Values(vec![Value::List(groups)]));
            }
            for (key, entry) in p.dict.entries() {
                if !matches!(key, "type" | "inGroups" | "nFaces" | "startFace") {
                    d.insert(key, entry.clone());
                }
            }
            let count = |n: usize| Entry::Values(vec![Value::Label(n as i64)]);
            d.insert("nFaces", count(p.n_faces));
            d.insert("startFace", count(p.start_face));
            let _ = writeln!(body, "    {}\n    {{", p.name);
            write::dictionary(&mut body, &d, 2);
            body.push_str("    }\n");
        }
        body.push_str(")\n");
        file("boundary", "polyBoundaryMesh", &body)
    }
}

fn labels(items: &[usize]) -> String {
    let mut out = format!("{}\n(\n", items.len());
    for v in items {
        let _ = writeln!(out, "{v}");
    }
    out.push_str(")\n");
    out
}

fn read_with<T>(path: &Path, read: impl FnOnce(&mut Tokenizer<'_>) -> Result<T>) -> Result<T> {
    let file = FoamFile::open(path)?;
    let (header, mut tok) = file.ascii_tokens().map_err(|e| e.in_file(&file.path))?;
    if header.is_some_and(|h| h.class == "faceCompactList") {
        return Err(FoamError::Unsupported("compact face lists".into()).in_file(&file.path));
    }
    let value = read(&mut tok).map_err(|e| e.in_file(&file.path))?;
    if let Some(t) = tok.next().map_err(|e| e.in_file(&file.path))? {
        return Err(FoamError::parse(
            t.line,
            format!("unexpected {} after the list", t.describe()),
        )
        .in_file(&file.path));
    }
    Ok(value)
}

/// The `boundary` file: `N( name { type …; nFaces …; startFace …; } … )`.
pub fn read_boundary(tok: &mut Tokenizer<'_>) -> Result<Vec<RawPatch>> {
    let first = tok.expect_any("the boundary list")?;
    let value = crate::dict::parse_value(first, tok, 0)?;
    let Value::List(items) = value else {
        return Err(FoamError::Invalid(
            "the boundary file does not hold a list".into(),
        ));
    };
    let mut patches = Vec::new();
    for item in items {
        let Value::Keyed(name, dict) = item else {
            return Err(FoamError::Invalid(
                "a boundary entry is not 'name { ... }'".into(),
            ));
        };
        let count = |key: &str| {
            dict.label(key)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| FoamError::Invalid(format!("patch '{name}' has no valid '{key}'")))
        };
        let groups = dict
            .value("inGroups")
            .and_then(Value::as_list)
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_word().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        patches.push(RawPatch {
            kind: dict
                .word("type")
                .ok_or_else(|| FoamError::Invalid(format!("patch '{name}' has no 'type'")))?
                .to_owned(),
            start_face: count("startFace")?,
            n_faces: count("nFaces")?,
            groups,
            name,
            dict,
        });
    }
    Ok(patches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_boundary_list() {
        let src = b"2( inlet { type patch; nFaces 3; startFace 10; } \
                     front { type wedge; inGroups 1(wedge); nFaces 4; startFace 13; } )";
        let p = read_boundary(&mut Tokenizer::new(src)).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            (p[1].kind.as_str(), p[1].start_face, p[1].n_faces),
            ("wedge", 13, 4)
        );
        assert_eq!(p[1].groups, vec!["wedge".to_owned()]);
    }

    #[test]
    fn a_patch_without_counts_is_an_error() {
        assert!(read_boundary(&mut Tokenizer::new(b"1( inlet { type patch; } )")).is_err());
        assert!(read_boundary(&mut Tokenizer::new(b"1( inlet )")).is_err());
        let untyped = b"1( inlet { nFaces 3; startFace 10; } )";
        assert!(read_boundary(&mut Tokenizer::new(untyped)).is_err());
    }

    #[test]
    fn a_written_mesh_reads_back_the_same() {
        let faces = FaceList {
            offsets: vec![0, 3, 6],
            points: vec![0, 1, 2, 0, 2, 3],
        };
        let mut dict = Dictionary::new();
        dict.insert(
            "neighbourPatch",
            Entry::Values(vec![Value::Word("b".into())]),
        );
        let mesh = RawPolyMesh {
            points: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.5e-9, 0.1, 3.0),
                Vec3::new(1.0, 1.0, -2.0),
                Vec3::new(0.0, 1.0, 1e12),
            ],
            faces,
            owner: vec![0, 0],
            neighbour: vec![],
            patches: vec![RawPatch {
                name: "walls".into(),
                kind: "wall".into(),
                start_face: 0,
                n_faces: 2,
                groups: vec!["wall".into()],
                dict,
            }],
        };
        let dir = tempfile::tempdir().unwrap();
        mesh.write_dir(dir.path()).unwrap();
        let back = RawPolyMesh::read_dir(dir.path()).unwrap();
        assert_eq!(back.points, mesh.points);
        assert_eq!(back.faces, mesh.faces);
        assert_eq!((back.owner, back.neighbour), (mesh.owner, mesh.neighbour));
        let p = &back.patches[0];
        assert_eq!(
            (p.name.as_str(), p.kind.as_str(), p.start_face, p.n_faces),
            ("walls", "wall", 0, 2)
        );
        assert_eq!(p.groups, vec!["wall".to_owned()]);
        assert_eq!(p.dict.word("neighbourPatch"), Some("b"));
    }

    #[test]
    fn a_file_is_its_list_and_nothing_more() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner");
        std::fs::write(&path, "2(0 1) // the end\n").unwrap();
        assert_eq!(read_with(&path, lists::read_labels).unwrap(), vec![0, 1]);
        std::fs::write(&path, "2(0 1) 7").unwrap();
        assert!(read_with(&path, lists::read_labels).is_err());
    }
}
