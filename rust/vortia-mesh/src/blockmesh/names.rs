//! The names a `blockMeshDict` may give to its vertices and blocks, and the
//! places that accept a name where a label goes.
//!
//! A vertex is named in the `vertices` list (`name inlet (0 0 0)`) or in a
//! `namedVertices` dictionary of its own (`inlet 0;`). Names stand for the
//! vertex labels of blocks, of edges and of the faces of `boundary` patches;
//! `namedBlocks` adds names for the faces' labels only. As in OpenFOAM's
//! `blockMeshTools::read`, a name that nothing defines is an error.

use std::collections::HashMap;

use vortia_foamfile::{Dictionary, Entry, Value};

use crate::error::{MeshError, Result};

fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Invalid(message.into()))
}

/// Name to label, for what a dictionary names.
#[derive(Debug, Clone, Default)]
pub(super) struct Names {
    vertices: HashMap<String, usize>,
    blocks: HashMap<String, usize>,
}

/// The labels a `namedVertices` or `namedBlocks` dictionary holds.
fn read_dict(dict: Option<&Dictionary>, into: &mut HashMap<String, usize>) -> Result<()> {
    let Some(dict) = dict else {
        return Ok(());
    };
    for (key, entry) in dict.entries() {
        let label = match entry {
            Entry::Values(v) => match v.as_slice() {
                [one] => one.as_label().and_then(|l| usize::try_from(l).ok()),
                _ => None,
            },
            _ => None,
        };
        let Some(label) = label else {
            return invalid(format!("the name {key} stands for a label"));
        };
        into.insert(key.to_owned(), label);
    }
    Ok(())
}

impl Names {
    /// The names a dictionary defines apart from the `vertices` list.
    pub(super) fn of(dict: &Dictionary) -> Result<Self> {
        let mut names = Self::default();
        read_dict(dict.dict("namedVertices"), &mut names.vertices)?;
        read_dict(dict.dict("namedBlocks"), &mut names.blocks)?;
        Ok(names)
    }

    /// Names vertex `label`. A name that is already taken keeps its first
    /// meaning, as in OpenFOAM, whose dictionary ignores the later entry
    /// (with a warning).
    pub(super) fn name_vertex(&mut self, name: &str, label: usize) {
        self.vertices.entry(name.to_owned()).or_insert(label);
    }

    fn find(&self, name: &str, for_faces: bool) -> Result<usize> {
        let found = self
            .vertices
            .get(name)
            .or_else(|| self.blocks.get(name).filter(|_| for_faces));
        found
            .copied()
            .map_or_else(|| invalid(format!("undefined variable {name}")), Ok)
    }

    /// A vertex label of a block or an edge: a number or a vertex's name.
    pub(super) fn vertex(&self, v: &Value, what: &str) -> Result<usize> {
        self.label(v, what, false)
    }

    /// A label of a patch face: a number, a vertex's or a block's name.
    pub(super) fn face_label(&self, v: &Value, what: &str) -> Result<usize> {
        self.label(v, what, true)
    }

    fn label(&self, v: &Value, what: &str, for_faces: bool) -> Result<usize> {
        match v {
            Value::Word(name) => self.find(name, for_faces),
            _ => v
                .as_label()
                .and_then(|n| usize::try_from(n).ok())
                .map_or_else(|| invalid(format!("{what}: expected a whole number")), Ok),
        }
    }
}
