//! The mesh topology: points, faces, and the cells on each side of a face.
//!
//! The layout is the face-addressed one OpenFOAM uses, so a case on disk maps
//! onto it without conversion:
//!
//! - every face has an *owner* cell, and the internal ones a *neighbour*;
//! - internal faces come first, then each boundary patch's faces in a block;
//! - a face's points are ordered so that its normal points out of its owner.

use std::path::Path;

use vortia_core::Vec3;
use vortia_foamfile::{FaceList, RawPolyMesh};

use crate::error::{MeshError, Result};

/// What a boundary patch is, which decides how the solver treats it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchKind {
    Patch,
    Wall,
    /// The unused direction of a 2D case.
    Empty,
    /// The two sides of an axisymmetric wedge.
    Wedge,
    SymmetryPlane,
    Symmetry,
    Cyclic,
    Processor,
    /// A type this engine does not know yet, kept by name.
    Other(String),
}

impl PatchKind {
    pub fn from_name(name: &str) -> Self {
        match name {
            "patch" => Self::Patch,
            "wall" => Self::Wall,
            "empty" => Self::Empty,
            "wedge" => Self::Wedge,
            "symmetryPlane" => Self::SymmetryPlane,
            "symmetry" => Self::Symmetry,
            "cyclic" => Self::Cyclic,
            "processor" => Self::Processor,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Patch => "patch",
            Self::Wall => "wall",
            Self::Empty => "empty",
            Self::Wedge => "wedge",
            Self::SymmetryPlane => "symmetryPlane",
            Self::Symmetry => "symmetry",
            Self::Cyclic => "cyclic",
            Self::Processor => "processor",
            Self::Other(name) => name,
        }
    }

    /// Patches that only reduce the dimension of the case, and carry no flow
    /// through them in any direction that matters.
    pub fn is_constraint_of_dimension(&self) -> bool {
        matches!(self, Self::Empty | Self::Wedge)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub name: String,
    pub kind: PatchKind,
    pub start: usize,
    pub size: usize,
    pub groups: Vec<String>,
}

impl Patch {
    pub fn faces(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.size
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolyMesh {
    points: Vec<Vec3>,
    faces: FaceList,
    owner: Vec<usize>,
    neighbour: Vec<usize>,
    patches: Vec<Patch>,
    n_cells: usize,
}

impl PolyMesh {
    /// Reads `<case>/constant/polyMesh` and checks it is a usable mesh.
    pub fn read_case(case: impl AsRef<Path>) -> Result<Self> {
        Self::from_raw(RawPolyMesh::read_case(case)?)
    }

    /// Builds a mesh from its parts, refusing any that do not fit together.
    ///
    /// What is refused is what would make later code index out of range or
    /// divide a cell between two meshes. Poor quality (skewed, non-orthogonal
    /// cells) is accepted here and reported by [`crate::check`].
    pub fn from_raw(raw: RawPolyMesh) -> Result<Self> {
        let RawPolyMesh {
            points,
            faces,
            owner,
            neighbour,
            patches,
        } = raw;
        let invalid = |m: String| Err(MeshError::Invalid(m));

        let n_faces = faces.len();
        if n_faces == 0 {
            return invalid("the mesh has no faces".into());
        }
        if owner.len() != n_faces {
            return invalid(format!("{} faces but {} owners", n_faces, owner.len()));
        }
        if neighbour.len() > n_faces {
            return invalid(format!(
                "{} neighbours for {} faces",
                neighbour.len(),
                n_faces
            ));
        }
        for i in 0..n_faces {
            let f = faces.face(i);
            if f.len() < 3 {
                return invalid(format!("face {i} has {} points", f.len()));
            }
            if let Some(&p) = f.iter().find(|&&p| p >= points.len()) {
                return invalid(format!("face {i} uses point {p}, of {}", points.len()));
            }
        }
        if let Some(p) = points
            .iter()
            .position(|p| !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()))
        {
            return invalid(format!("point {p} is not a finite position"));
        }

        let n_cells = owner.iter().chain(&neighbour).max().map_or(0, |&m| m + 1);
        for (i, (&o, &n)) in owner.iter().zip(&neighbour).enumerate() {
            if o == n {
                return invalid(format!("internal face {i} has cell {o} on both sides"));
            }
        }

        let n_internal = neighbour.len();
        let mut next = n_internal;
        let mut out = Vec::with_capacity(patches.len());
        for p in patches {
            if p.start_face != next {
                return invalid(format!(
                    "patch '{}' starts at face {}, expected {next}",
                    p.name, p.start_face
                ));
            }
            next = p
                .start_face
                .checked_add(p.n_faces)
                .filter(|&end| end <= n_faces)
                .ok_or_else(|| {
                    MeshError::Invalid(format!("patch '{}' runs past the last face", p.name))
                })?;
            out.push(Patch {
                kind: PatchKind::from_name(&p.kind),
                name: p.name,
                start: p.start_face,
                size: p.n_faces,
                groups: p.groups,
            });
        }
        if next != n_faces {
            return invalid(format!("faces {next}..{n_faces} belong to no patch"));
        }

        Ok(Self {
            points,
            faces,
            owner,
            neighbour,
            patches: out,
            n_cells,
        })
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn n_points(&self) -> usize {
        self.points.len()
    }

    pub fn n_faces(&self) -> usize {
        self.faces.len()
    }

    pub fn n_internal_faces(&self) -> usize {
        self.neighbour.len()
    }

    pub fn n_cells(&self) -> usize {
        self.n_cells
    }

    pub fn faces(&self) -> &FaceList {
        &self.faces
    }

    /// The points of face `i`; empty for an index out of range.
    pub fn face(&self, i: usize) -> &[usize] {
        self.faces.face(i)
    }

    pub fn owner(&self) -> &[usize] {
        &self.owner
    }

    pub fn neighbour(&self) -> &[usize] {
        &self.neighbour
    }

    pub fn patches(&self) -> &[Patch] {
        &self.patches
    }

    pub fn patch(&self, name: &str) -> Option<&Patch> {
        self.patches.iter().find(|p| p.name == name)
    }

    /// The point positions of face `i`, in order.
    pub fn face_points(&self, i: usize) -> impl Iterator<Item = Vec3> + '_ {
        self.face(i)
            .iter()
            .filter_map(|&p| self.points.get(p).copied())
    }
}
