//! The `system/` and `constant/` dictionaries a solver reads.

use std::path::{Path, PathBuf};

use vortia_foamfile::{Dictionary, Entry, FoamFile, Value};
use vortia_fv::ops::{Convection, SnGrad};
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
        .all(|(i, &e)| (numbers.get(i).copied().unwrap_or(0.0) - f64::from(e)).abs() < 1e-10);
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
        let missing = || SolverError::Setup(format!("fvSchemes/{section} has no '{key}'"));
        let values = match d.values(key) {
            Some(v) => v,
            None => {
                let default = d.values("default").ok_or_else(missing)?;
                // `default none` asks for every key to be given.
                if default.iter().filter_map(|v| v.as_word()).eq(["none"]) {
                    return Err(missing());
                }
                default
            }
        };
        Ok(values
            .iter()
            .filter_map(|v| v.as_word().map(str::to_owned))
            .collect())
    }

    /// The convection scheme `divSchemes` gives for `key`.
    pub fn convection(&self, key: &str) -> Result<Convection> {
        let w = self.scheme("divSchemes", key)?;
        Ok(Convection::from_scheme(&words(&w))?)
    }

    /// The surface-normal gradient of `laplacianSchemes` for `key`, refusing
    /// a non-orthogonal correction, which the solvers do not have yet.
    pub fn uncorrected_sn_grad(&self, key: &str) -> Result<SnGrad> {
        let w = self.scheme("laplacianSchemes", key)?;
        let s = SnGrad::from_scheme(&words(&w))?;
        if s == SnGrad::Corrected {
            return Err(SolverError::Setup(format!(
                "laplacian scheme '{}' for {key} is not supported yet; \
                 use Gauss linear orthogonal or uncorrected",
                w.join(" ")
            )));
        }
        Ok(s)
    }

    /// Refuses a `gradSchemes` entry for `grad(field)` other than
    /// `Gauss linear`, the gradient the engine has.
    pub fn require_gauss_linear_grad(&self, field: &str) -> Result<()> {
        let w = self.scheme("gradSchemes", &format!("grad({field})"))?;
        if w != ["Gauss", "linear"] {
            return Err(SolverError::Setup(format!(
                "grad scheme '{}' is not supported yet; use Gauss linear",
                w.join(" ")
            )));
        }
        Ok(())
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
            // Every value has to be a word: `linear (upwind)` is not linear.
            let words: Vec<&str> = match entry {
                vortia_foamfile::Entry::Values(v) => {
                    v.iter().map(|v| v.as_word().unwrap_or("(…)")).collect()
                }
                _ => vec!["{…}"],
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
        expand(
            d,
            &find,
            0,
            &mut Budget {
                entries: MAX_EXPANDED_ENTRIES,
                weight: MAX_EXPANDED_WEIGHT,
            },
        )
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

/// The most entries `$` references may expand to in one lookup: depth
/// alone does not bound a dictionary that names another many times over.
const MAX_EXPANDED_ENTRIES: usize = 10_000;
/// The most the copies one lookup makes may weigh (see [`weight`]): a few
/// references to one huge value would otherwise copy it into memory many
/// times over while staying well under the entry count.
const MAX_EXPANDED_WEIGHT: usize = 1 << 22;

/// Roughly the bytes an entry holds: one per value plus its text.
fn weight(entry: &Entry) -> usize {
    fn value(v: &Value) -> usize {
        match v {
            Value::Word(s) | Value::Str(s) => 1 + s.len(),
            Value::Label(_) | Value::Scalar(_) => 1,
            Value::List(l) | Value::Dimensions(l) => 1 + l.iter().map(value).sum::<usize>(),
            Value::Dict(d) => 1 + dict(d),
            Value::Keyed(k, d) => 1 + k.len() + dict(d),
        }
    }
    fn dict(d: &Dictionary) -> usize {
        d.entries().map(|(k, e)| k.len() + weight(e)).sum()
    }
    match entry {
        Entry::Dict(d) => 1 + dict(d),
        Entry::Values(v) | Entry::Directive(v) => 1 + v.iter().map(value).sum::<usize>(),
    }
}

struct Budget {
    entries: usize,
    weight: usize,
}

/// `d` with each `$name` entry replaced by the entries of `find(name)`.
fn expand<'a>(
    d: &Dictionary,
    find: &impl Fn(&str) -> Option<&'a Dictionary>,
    depth: u32,
    budget: &mut Budget,
) -> Result<Dictionary> {
    if depth > 8 {
        return Err(SolverError::Setup(
            "fvSolution: '$' references nest too deep".into(),
        ));
    }
    let mut out = Dictionary::new();
    for (key, entry) in d.entries() {
        budget.entries = budget.entries.checked_sub(1).ok_or_else(|| {
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
                    out.set(k, e.clone());
                }
            }
            None => {
                budget.weight = budget.weight.checked_sub(weight(entry)).ok_or_else(|| {
                    SolverError::Setup("fvSolution: '$' references copy too much data".into())
                })?;
                out.set(key, entry.clone());
            }
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
    // `%g` picks its style by the exponent after rounding to `p` digits:
    // 999999.7 is `1e+06`, not `1000000`.
    let digits = usize::try_from(p - 1).unwrap_or(0);
    let sci = format!("{t:.digits$e}");
    let (mant, e) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = e.parse().unwrap_or(0);
    if !(-4..p).contains(&exp) {
        let mant = trim(mant);
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", exp.abs());
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

fn words(w: &[String]) -> Vec<&str> {
    w.iter().map(String::as_str).collect()
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
        // Repeating one reference keeps one copy of what it names, not one
        // per repeat, so a large value named many times stays one value.
        let d = case.solver_dict("l2").unwrap();
        assert_eq!(d.entries().count(), 1);
    }

    #[test]
    fn references_to_a_large_value_are_bounded_by_size() {
        // A 1 MB word named 1000 times: few entries, a gigabyte of copies.
        let big = "w".repeat(1 << 20);
        let refs: String = (0..1000).map(|i| format!("r{i} {{ $big; }} ")).collect();
        let all: String = (0..1000).map(|i| format!("$r{i}; ")).collect();
        let text = format!("solvers {{ big {{ v {big}; }} {refs} all {{ {all} }} }}");
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("system");
        std::fs::create_dir_all(&system).unwrap();
        for f in ["controlDict", "fvSchemes"] {
            std::fs::write(system.join(f), "a 1;").unwrap();
        }
        std::fs::write(system.join("fvSolution"), text).unwrap();
        let case = super::Case::open(dir.path()).unwrap();
        assert!(case.solver_dict("r0").is_ok());
        let err = case.solver_dict("all").unwrap_err().to_string();
        assert!(err.contains("too much data"), "{err}");
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
        // The style follows the rounded value, as C's `%g` does.
        assert_eq!(time_name(999999.7), "1e+06");
        assert_eq!(time_name(0.00009999996), "0.0001");
    }
}
