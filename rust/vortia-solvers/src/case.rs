//! The `system/` and `constant/` dictionaries a solver reads.

use std::path::{Path, PathBuf};

use vortia_foamfile::{Dictionary, Entry, FoamFile, Value};
use vortia_fv::{Controls, FvGeometry};
use vortia_mesh::PolyMesh;

use crate::error::{Result, SolverError};

/// A case directory and its dictionaries.
pub struct Case {
    pub dir: PathBuf,
    pub control: Dictionary,
    pub schemes: Dictionary,
    pub solution: Dictionary,
}

/// A dictionary the solver can use as written: one with a directive in it
/// (`#include`, `#calc`, …) is refused, since what it would have changed is
/// not known until it is evaluated, and the engine does not evaluate them yet.
pub fn read_dict(path: &Path) -> Result<Dictionary> {
    let d = FoamFile::open(path)?.dictionary()?.1;
    match first_directive(&d) {
        Some(key) => Err(SolverError::Setup(format!(
            "{}: '{key}' is not supported yet",
            path.display()
        ))),
        None => Ok(d),
    }
}

/// SI exponents: kg, m, s, K, mol, A, cd.
pub type Dims = [i32; 7];
pub const KINEMATIC_DIFFUSIVITY: Dims = [0, 2, -1, 0, 0, 0, 0];
pub const VELOCITY: Dims = [0, 1, -1, 0, 0, 0, 0];
pub const VOLUME_FLUX: Dims = [0, 3, -1, 0, 0, 0, 0];
pub const KINEMATIC_PRESSURE: Dims = [0, 2, -2, 0, 0, 0, 0];

/// Refuses `value` unless it is the dimension set `expected`, written as
/// five or seven numbers as OpenFOAM accepts it.
pub fn check_dimensions(what: &str, value: &Value, expected: Dims) -> Result<()> {
    let wrong = || {
        SolverError::Setup(format!(
            "{what} has dimensions {}, expected {}",
            shown_dims(value),
            format_dims(expected)
        ))
    };
    let Value::Dimensions(items) = value else {
        return Err(wrong());
    };
    let numbers: Option<Vec<f64>> = items.iter().map(Value::as_scalar).collect();
    let numbers = numbers
        .filter(|n| n.len() == 5 || n.len() == 7)
        .ok_or_else(wrong)?;
    let same = expected
        .iter()
        .enumerate()
        .all(|(i, &e)| numbers.get(i).copied().unwrap_or(0.0) == f64::from(e));
    if same { Ok(()) } else { Err(wrong()) }
}

fn format_dims(d: Dims) -> String {
    let parts: Vec<String> = d.iter().map(i32::to_string).collect();
    format!("[{}]", parts.join(" "))
}

fn shown_dims(value: &Value) -> String {
    match value {
        Value::Dimensions(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|v| match v {
                    Value::Word(w) => w.clone(),
                    other => other
                        .as_scalar()
                        .map_or_else(|| "?".into(), |x| x.to_string()),
                })
                .collect();
            format!("[{}]", parts.join(" "))
        }
        _ => "none".into(),
    }
}

fn first_directive(d: &Dictionary) -> Option<&str> {
    d.entries().find_map(|(k, e)| match e {
        Entry::Directive(_) => Some(k),
        Entry::Dict(inner) => first_directive(inner),
        Entry::Values(_) => None,
    })
}

impl Case {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_owned();
        let system = dir.join("system");
        Ok(Self {
            control: read_dict(&system.join("controlDict"))?,
            schemes: read_dict(&system.join("fvSchemes"))?,
            solution: read_dict(&system.join("fvSolution"))?,
            dir,
        })
    }

    /// The case's mesh with its geometry, refused when it cannot be solved
    /// on: a flipped face or a collapsed cell would otherwise give
    /// coefficients that look valid and a result that is not.
    pub fn mesh(&self) -> Result<(PolyMesh, FvGeometry)> {
        let mesh = PolyMesh::read_case(&self.dir)?;
        let fv = FvGeometry::new(&mesh);
        let problems = vortia_mesh::check::check(&mesh, &fv.geo).unsolvable();
        if !problems.is_empty() {
            return Err(SolverError::Setup(format!(
                "the mesh cannot be solved on: {}",
                problems.join(", ")
            )));
        }
        Ok((mesh, fv))
    }

    pub fn application(&self) -> Option<&str> {
        self.control.word("application")
    }

    pub fn scalar(&self, key: &str) -> Result<f64> {
        self.control
            .scalar(key)
            .ok_or_else(|| SolverError::Setup(format!("controlDict has no number '{key}'")))
    }

    /// A scheme entry as words: the field's own key, else `default`.
    pub fn scheme(&self, section: &str, key: &str) -> Result<Vec<String>> {
        let d = self
            .schemes
            .dict(section)
            .ok_or_else(|| SolverError::Setup(format!("fvSchemes has no '{section}'")))?;
        let values = d
            .values(key)
            .or_else(|| d.values("default"))
            .ok_or_else(|| {
                SolverError::Setup(format!("fvSchemes/{section} has no '{key}' or 'default'"))
            })?;
        Ok(values
            .iter()
            .filter_map(|v| v.as_word().map(str::to_owned))
            .collect())
    }

    /// Refuses `interpolationSchemes` other than `linear`, the one the
    /// engine's face interpolation is: `interpolate(rAU)` and `flux(HbyA)`
    /// read their own keys there, so every entry is checked, not only
    /// `default`.
    pub fn require_linear_interpolation(&self) -> Result<()> {
        let Some(d) = self.schemes.dict("interpolationSchemes") else {
            return Ok(());
        };
        for (key, entry) in d.entries() {
            let words: Vec<&str> = match entry {
                vortia_foamfile::Entry::Values(v) => v.iter().filter_map(|v| v.as_word()).collect(),
                _ => Vec::new(),
            };
            if words != ["linear"] {
                return Err(SolverError::Setup(format!(
                    "interpolation scheme '{}' for {key} is not supported yet; use linear",
                    words.join(" ")
                )));
            }
        }
        Ok(())
    }

    /// The linear-solver settings for `field`, by exact key then pattern.
    pub fn solver_controls(&self, field: &str) -> Result<(String, String, Controls)> {
        let d = self.solver_dict(field)?;
        let defaults = Controls::default();
        let count = |k: &str, def: usize| {
            d.label(k)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(def)
        };
        let controls = Controls {
            tolerance: d.scalar("tolerance").unwrap_or(defaults.tolerance),
            rel_tol: d.scalar("relTol").unwrap_or(defaults.rel_tol),
            max_iter: count("maxIter", defaults.max_iter),
            min_iter: count("minIter", defaults.min_iter),
        };
        // `preconditioner DIC;` or `preconditioner { preconditioner DIC; … }`.
        let precon = d
            .dict("preconditioner")
            .and_then(|p| p.word("preconditioner"))
            .or_else(|| d.word("preconditioner"))
            .or_else(|| d.word("smoother"))
            .unwrap_or("none")
            .to_owned();
        let solver = d
            .word("solver")
            .ok_or_else(|| SolverError::Setup(format!("no 'solver' for '{field}'")))?;
        Ok((solver.to_owned(), precon, controls))
    }

    /// The `fvSolution/solvers` entry for `field`, by exact key then
    /// pattern, with `$other` lines replaced by that entry's contents (what
    /// `pFinal { $p; relTol 0; }` means).
    pub fn solver_dict(&self, field: &str) -> Result<Dictionary> {
        let solvers = self
            .solution
            .dict("solvers")
            .ok_or_else(|| SolverError::Setup("fvSolution has no 'solvers'".into()))?;
        let find = |name: &str| {
            solvers.dict(name).or_else(|| {
                solvers.entries().rev().find_map(|(k, e)| match e {
                    Entry::Dict(d) if key_matches(k, name) => Some(d),
                    _ => None,
                })
            })
        };
        let d = find(field).ok_or_else(|| {
            SolverError::Setup(format!("fvSolution/solvers has no entry for '{field}'"))
        })?;
        expand(d, &find, 0, &mut { MAX_EXPANDED_ENTRIES })
    }

    pub fn non_orthogonal_correctors(&self, algorithm: &str) -> usize {
        self.solution
            .dict(algorithm)
            .and_then(|d| d.label("nNonOrthogonalCorrectors"))
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0)
            .min(100)
    }
}

/// `d` with each `$name` entry replaced by the entries of `find(name)`.
/// The most entries `$` references may expand to in one lookup: depth
/// alone does not bound a dictionary that names another many times over.
const MAX_EXPANDED_ENTRIES: usize = 10_000;

fn expand<'a>(
    d: &Dictionary,
    find: &impl Fn(&str) -> Option<&'a Dictionary>,
    depth: u32,
    budget: &mut usize,
) -> Result<Dictionary> {
    if depth > 8 {
        return Err(SolverError::Setup(
            "fvSolution: '$' references nest too deep".into(),
        ));
    }
    let mut out = Dictionary::new();
    for (key, entry) in d.entries() {
        *budget = budget.checked_sub(1).ok_or_else(|| {
            SolverError::Setup(format!(
                "fvSolution: '$' references expand to more than {MAX_EXPANDED_ENTRIES} entries"
            ))
        })?;
        match key.strip_prefix('$') {
            Some(name) => {
                let other = find(name).ok_or_else(|| {
                    SolverError::Setup(format!("fvSolution/solvers has no '{name}' for '${name}'"))
                })?;
                for (k, e) in expand(other, find, depth + 1, budget)?.entries() {
                    out.insert(k, e.clone());
                }
            }
            None => out.insert(key, entry.clone()),
        }
    }
    Ok(out)
}

pub(crate) fn key_matches(pattern: &str, name: &str) -> bool {
    let inner = pattern
        .strip_prefix('(')
        .and_then(|p| p.strip_suffix(')'))
        .unwrap_or(pattern);
    inner.split('|').any(|alt| match alt.strip_suffix(".*") {
        Some(prefix) => name.starts_with(prefix),
        None => alt == name,
    })
}

/// A time as OpenFOAM names its directory: `%g` with 6 significant digits.
pub fn time_name(t: f64) -> String {
    time_name_with(t, DEFAULT_TIME_PRECISION)
}

/// `timePrecision` when `controlDict` gives none.
pub const DEFAULT_TIME_PRECISION: u32 = 6;
/// More digits than an `f64` holds say nothing.
pub const MAX_TIME_PRECISION: u32 = 17;

/// The name OpenFOAM gives time `t` with `precision` significant digits
/// (C's `%g`).
pub fn time_name_with(t: f64, precision: u32) -> String {
    if t == 0.0 {
        return "0".into();
    }
    let p = precision.clamp(1, MAX_TIME_PRECISION) as i32;
    let exp = t.abs().log10().floor() as i32;
    if !(-4..p).contains(&exp) {
        let digits = usize::try_from(p - 1).unwrap_or(0);
        let s = format!("{t:.digits$e}");
        let (mant, e) = s.split_once('e').unwrap_or((&s, "0"));
        let mant = trim(mant);
        let e: i32 = e.parse().unwrap_or(0);
        let sign = if e < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", e.abs());
    }
    let decimals = usize::try_from(p - 1 - exp).unwrap_or(0);
    trim(&format!("{t:.decimals$}"))
}

fn trim(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{time_name, time_name_with};

    #[test]
    fn references_that_fan_out_are_bounded() {
        // Each level names the one below ten times: 10^8 entries at the end.
        let mut text = String::from("solvers { l0 { tolerance 1; } ");
        for level in 1..=8 {
            let refs: String = (0..10).map(|_| format!("$l{}; ", level - 1)).collect();
            text.push_str(&format!("l{level} {{ {refs} }} "));
        }
        text.push('}');
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("system");
        std::fs::create_dir_all(&system).unwrap();
        for f in ["controlDict", "fvSchemes"] {
            std::fs::write(system.join(f), "a 1;").unwrap();
        }
        std::fs::write(system.join("fvSolution"), text).unwrap();
        let case = super::Case::open(dir.path()).unwrap();
        assert!(case.solver_dict("l1").is_ok());
        let err = case.solver_dict("l8").unwrap_err().to_string();
        assert!(err.contains("more than"), "{err}");
    }

    #[test]
    fn time_names_match_openfoam() {
        for (t, name) in [
            (0.0, "0"),
            (1.0, "1"),
            (0.005, "0.005"),
            (100.0, "100"),
            (0.1 + 0.2, "0.3"),
            (1e-5, "1e-05"),
            (1234567.0, "1.23457e+06"),
        ] {
            assert_eq!(time_name(t), name);
        }
        assert_eq!(time_name_with(50000.01, 6), "50000");
        assert_eq!(time_name_with(50000.01, 7), "50000.01");
        assert_eq!(time_name_with(1234567.0, 8), "1234567");
    }
}
