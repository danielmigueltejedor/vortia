//! A field: values on the cells (or faces) of a mesh, and on each patch.

use std::path::Path;

use vortia_foamfile::{Dictionary, Entry, FoamFile, Value, write};
use vortia_mesh::{Patch, PatchKind, PolyMesh};

use crate::data::Data;
use crate::error::{FieldError, Result};
use crate::value::FieldValue;

/// Where a field's internal values live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// One value per cell: `volScalarField`, `volVectorField`.
    Cells,
    /// One value per internal face: `surfaceScalarField` (fluxes).
    Faces,
}

impl Location {
    fn prefix(self) -> &'static str {
        match self {
            Location::Cells => "vol",
            Location::Faces => "surface",
        }
    }

    pub fn class<T: FieldValue>(self) -> String {
        format!("{}{}Field", self.prefix(), T::CLASS_NAME)
    }
}

/// One entry of `boundaryField`.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchField<T> {
    /// The patch name, or a pattern / group it applies to.
    pub key: String,
    /// The condition: `fixedValue`, `zeroGradient`, `wedge`, …
    pub kind: String,
    pub value: Option<Data<T>>,
    /// Every other entry, in order, so the condition writes back unchanged.
    pub settings: Dictionary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field<T> {
    pub name: String,
    pub location: Location,
    pub dimensions: Value,
    pub internal: Data<T>,
    pub boundary: Vec<PatchField<T>>,
    /// Directives inside `boundaryField` (`#includeEtc "caseDicts/…"`), kept
    /// and written back ahead of the patches.
    pub boundary_directives: Dictionary,
    /// Other top-level entries (`oriented`, directives), kept as they were.
    pub extra: Dictionary,
}

impl<T: FieldValue> Field<T> {
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let file = FoamFile::open(path.as_ref())?;
        Self::from_file(&file).map_err(|e| match e {
            FieldError::Invalid(m) => FieldError::Invalid(format!("{}: {m}", file.path.display())),
            other => other,
        })
    }

    pub fn from_file(file: &FoamFile) -> Result<Self> {
        let (header, dict) = file.dictionary()?;
        let header = header.ok_or_else(|| FieldError::Invalid("no FoamFile header".into()))?;
        let location = [Location::Cells, Location::Faces]
            .into_iter()
            .find(|l| l.class::<T>() == header.class)
            .ok_or_else(|| {
                FieldError::Invalid(format!(
                    "a {} cannot be read as a {} field",
                    header.class,
                    T::NAME
                ))
            })?;
        let name = if header.object.is_empty() {
            file.path
                .file_name()
                .map(|n| n.to_string_lossy().trim_end_matches(".gz").to_owned())
                .unwrap_or_default()
        } else {
            header.object.clone()
        };
        if !is_file_name(&name) {
            return Err(FieldError::Invalid(format!(
                "field name {name:?} is not a plain file name"
            )));
        }
        Self::from_dictionary(name, location, &dict)
    }

    fn from_dictionary(name: String, location: Location, dict: &Dictionary) -> Result<Self> {
        let invalid = |m: &str| FieldError::Invalid(m.to_owned());
        let dimensions = dict
            .value("dimensions")
            .cloned()
            .ok_or_else(|| invalid("no 'dimensions'"))?;
        let internal = Data::parse(
            dict.values("internalField")
                .ok_or_else(|| invalid("no 'internalField'"))?,
            "internalField",
        )?;
        let boundary_dict = dict
            .dict("boundaryField")
            .ok_or_else(|| invalid("no 'boundaryField'"))?;

        let mut boundary = Vec::new();
        let mut boundary_directives = Dictionary::new();
        for (key, entry) in boundary_dict.entries() {
            let d = match entry {
                Entry::Dict(d) => d,
                Entry::Directive(_) => {
                    boundary_directives.insert(key, entry.clone());
                    continue;
                }
                Entry::Values(_) => continue,
            };
            if key.is_empty() || key.contains('"') || key.chars().any(char::is_control) {
                return Err(FieldError::Invalid(format!(
                    "boundaryField key {key:?} cannot be written back"
                )));
            }
            let kind = d
                .word("type")
                .filter(|k| is_word(k))
                .ok_or_else(|| FieldError::Invalid(format!("patch '{key}' has no valid 'type'")))?
                .to_owned();
            let value = d
                .values("value")
                .map(|v| Data::parse(v, &format!("patch '{key}' value")))
                .transpose()?;
            let mut settings = Dictionary::new();
            for (k, e) in d.entries().filter(|(k, _)| *k != "type" && *k != "value") {
                settings.insert(k, e.clone());
            }
            boundary.push(PatchField {
                key: key.to_owned(),
                kind,
                value,
                settings,
            });
        }

        let mut extra = Dictionary::new();
        for (k, e) in dict.entries() {
            if !matches!(k, "dimensions" | "internalField" | "boundaryField") {
                extra.insert(k, e.clone());
            }
        }
        Ok(Self {
            name,
            location,
            dimensions,
            internal,
            boundary,
            boundary_directives,
            extra,
        })
    }

    /// The text of the file, as OpenFOAM writes it.
    pub fn to_text(&self, time: &str) -> String {
        let mut out = String::new();
        write::header(
            &mut out,
            &self.location.class::<T>(),
            Some(time),
            &self.name,
        );
        out.push_str("dimensions      ");
        write::value(&mut out, &self.dimensions, 0);
        out.push_str(";\n\n");
        if self.extra.entries().next().is_some() {
            write::dictionary(&mut out, &self.extra, 0);
            out.push('\n');
        }
        out.push_str("internalField   ");
        self.internal.write(&mut out);
        out.push_str(";\n\nboundaryField\n{\n");
        if self.boundary_directives.entries().next().is_some() {
            write::dictionary(&mut out, &self.boundary_directives, 1);
            out.push('\n');
        }
        for p in &self.boundary {
            let key = write::key_text(&p.key);
            out.push_str(&format!(
                "    {key}\n    {{\n        type            {};\n",
                p.kind
            ));
            write::dictionary(&mut out, &p.settings, 2);
            if let Some(v) = &p.value {
                out.push_str("        value           ");
                v.write(&mut out);
                out.push_str(";\n");
            }
            out.push_str("    }\n");
        }
        out.push_str("}\n");
        write::footer(&mut out);
        out
    }

    /// Writes the field into `case/time/name`, atomically.
    pub fn write(&self, case: impl AsRef<Path>, time: &str) -> Result<()> {
        // Both come from case files; neither may lead outside `case`.
        for part in [time, self.name.as_str()] {
            if !is_file_name(part) {
                return Err(FieldError::Invalid(format!(
                    "{part:?} is not a plain file name"
                )));
            }
        }
        // The header holds both: the name bare, the time quoted.
        if !is_word(&self.name) || time.contains(['"', '\\']) {
            return Err(FieldError::Invalid(format!(
                "field {:?} at time {time:?} would not read back",
                self.name
            )));
        }
        let dir = case.as_ref().join(time);
        // A time directory that is a link would take the write elsewhere.
        if dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(FieldError::Invalid(format!(
                "time directory {time:?} is a link; not writing through it"
            )));
        }
        let path = dir.join(&self.name);
        write::atomic(&path, self.to_text(time).as_bytes())?;
        Ok(())
    }

    /// The first directive the field holds that would have to be evaluated
    /// to know its conditions, if any. `#includeEtc
    /// "caseDicts/setConstraintTypes"` is not one: it only adds entries
    /// for constraint groups, which a patch's own entry or a pattern
    /// already decides, and a patch left to it has no condition here and is
    /// refused as such.
    pub fn unevaluated_directive(&self) -> Option<String> {
        let constraint_types = |key: &str, e: &Entry| {
            key == "#includeEtc"
                && matches!(e, Entry::Directive(v)
                    if v.len() == 1
                        && v.first().and_then(Value::as_word) == Some("caseDicts/setConstraintTypes"))
        };
        let directive = |d: &Dictionary, allow: &dyn Fn(&str, &Entry) -> bool| {
            d.entries()
                .find(|(k, e)| matches!(e, Entry::Directive(_)) && !allow(k, e))
                .map(|(k, _)| k.to_owned())
        };
        let none = |_: &str, _: &Entry| false;
        directive(&self.extra, &none)
            .or_else(|| directive(&self.boundary_directives, &constraint_types))
            .or_else(|| {
                self.boundary
                    .iter()
                    .find_map(|pf| directive(&pf.settings, &none))
            })
    }

    /// Drops the `value` of every `zeroGradient` and `fixedGradient` entry,
    /// as OpenFOAM v1912 writes them: a value read with the field goes stale
    /// once the cells change, and a reader that finds one would show it
    /// instead of the cells'.
    pub fn drop_zero_gradient_values(&mut self) {
        for pf in &mut self.boundary {
            if matches!(pf.kind.as_str(), "zeroGradient" | "fixedGradient") {
                pf.value = None;
            }
        }
    }

    /// The entry that applies to `patch`: its own name first, then a
    /// pattern, then one of its groups, which is the order OpenFOAM uses.
    pub fn patch_field(&self, patch: &Patch) -> Option<&PatchField<T>> {
        let by = |f: &dyn Fn(&str) -> bool| self.boundary.iter().rev().find(|p| f(&p.key));
        by(&|k| k == patch.name)
            .or_else(|| by(&|k| pattern_matches(k, &patch.name)))
            .or_else(|| by(&|k| patch.groups.iter().any(|g| g == k) || k == patch.kind.name()))
    }

    /// How many internal values the field must have on `mesh`.
    pub fn internal_len(&self, mesh: &PolyMesh) -> usize {
        match self.location {
            Location::Cells => mesh.n_cells(),
            Location::Faces => mesh.n_internal_faces(),
        }
    }

    /// Checks the field fits `mesh`: as many values as cells (or internal
    /// faces), and a condition with the right number of values for every patch.
    pub fn check_against(&self, mesh: &PolyMesh) -> Result<()> {
        let mismatch = |message: String| FieldError::Mismatch {
            field: self.name.clone(),
            message,
        };
        let n = self.internal_len(mesh);
        if let Some(len) = self.internal.count().filter(|&len| len != n) {
            return Err(mismatch(format!("{len} internal values for {n}")));
        }
        for patch in mesh.patches() {
            let pf = self
                .patch_field(patch)
                .ok_or_else(|| mismatch(format!("no condition for patch '{}'", patch.name)))?;
            let expected = if patch.kind == PatchKind::Empty {
                0
            } else {
                patch.size
            };
            if let Some(len) = pf.value.as_ref().and_then(Data::count) {
                // An empty patch may also carry a value for its (zero) faces.
                if len != expected && len != patch.size {
                    return Err(mismatch(format!(
                        "{len} values on patch '{}' of {} faces",
                        patch.name, patch.size
                    )));
                }
            }
        }
        Ok(())
    }
}

/// The patterns case files use as patch keys: `.*`, `(a|b|c)`, `prefix.*`.
/// Anything else matches only itself.
/// One path component that stays where it is joined: no separators, not `.`
/// or `..`, nothing a terminal or a shell would read as more.
fn is_file_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains(['/', '\\'])
        && !s.chars().any(char::is_control)
}

/// A word the case format reads back as one token.
fn is_word(s: &str) -> bool {
    !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ';' | '{' | '}'))
}

fn pattern_matches(pattern: &str, name: &str) -> bool {
    let inner = pattern
        .strip_prefix('(')
        .and_then(|p| p.strip_suffix(')'))
        .unwrap_or(pattern);
    inner.split('|').any(|alt| match alt.strip_suffix(".*") {
        Some(prefix) => name.starts_with(prefix),
        None => alt == name,
    })
}

#[cfg(test)]
mod tests {
    use super::pattern_matches;

    #[test]
    fn patterns() {
        assert!(pattern_matches(".*", "inlet"));
        assert!(pattern_matches("(front|back)", "back"));
        assert!(pattern_matches("wall.*", "walls"));
        assert!(!pattern_matches("(front|back)", "inlet"));
    }
}
