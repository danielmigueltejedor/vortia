//! The five files of `constant/polyMesh`, read as they are on disk.
//!
//! Nothing here checks that the files agree with each other: that is the
//! mesh's job (`vortia-mesh`), which knows what a valid mesh is.

use std::path::Path;

use vortia_core::Vec3;

use crate::dict::{Dictionary, Value};
use crate::error::{FoamError, Result};
use crate::file::FoamFile;
use crate::lists::{self, FaceList};
use crate::token::Tokenizer;

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
    fn a_file_is_its_list_and_nothing_more() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner");
        std::fs::write(&path, "2(0 1) // the end\n").unwrap();
        assert_eq!(read_with(&path, lists::read_labels).unwrap(), vec![0, 1]);
        std::fs::write(&path, "2(0 1) 7").unwrap();
        assert!(read_with(&path, lists::read_labels).is_err());
    }
}
