//! The `system/` and `constant/` dictionaries a solver reads.

use std::path::{Path, PathBuf};

use vortia_foamfile::{Dictionary, FoamFile};
use vortia_fv::Controls;

use crate::error::{Result, SolverError};

/// A case directory and its dictionaries.
pub struct Case {
    pub dir: PathBuf,
    pub control: Dictionary,
    pub schemes: Dictionary,
    pub solution: Dictionary,
}

pub fn read_dict(path: &Path) -> Result<Dictionary> {
    Ok(FoamFile::open(path)?.dictionary()?.1)
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
                    vortia_foamfile::Entry::Dict(d) if key_matches(k, name) => Some(d),
                    _ => None,
                })
            })
        };
        let d = find(field).ok_or_else(|| {
            SolverError::Setup(format!("fvSolution/solvers has no entry for '{field}'"))
        })?;
        expand(d, &find, 0)
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
fn expand<'a>(
    d: &Dictionary,
    find: &impl Fn(&str) -> Option<&'a Dictionary>,
    depth: u32,
) -> Result<Dictionary> {
    if depth > 8 {
        return Err(SolverError::Setup(
            "fvSolution: '$' references nest too deep".into(),
        ));
    }
    let mut out = Dictionary::new();
    for (key, entry) in d.entries() {
        match key.strip_prefix('$') {
            Some(name) => {
                let other = find(name).ok_or_else(|| {
                    SolverError::Setup(format!("fvSolution/solvers has no '{name}' for '${name}'"))
                })?;
                for (k, e) in expand(other, find, depth + 1)?.entries() {
                    out.insert(k, e.clone());
                }
            }
            None => out.insert(key, entry.clone()),
        }
    }
    Ok(out)
}

fn key_matches(pattern: &str, name: &str) -> bool {
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
    if t == 0.0 {
        return "0".into();
    }
    let exp = t.abs().log10().floor() as i32;
    if !(-4..6).contains(&exp) {
        let s = format!("{t:.5e}");
        let (mant, e) = s.split_once('e').unwrap_or((&s, "0"));
        let mant = trim(mant);
        let e: i32 = e.parse().unwrap_or(0);
        let sign = if e < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", e.abs());
    }
    let decimals = usize::try_from(5 - exp).unwrap_or(0);
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
    use super::time_name;

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
    }
}
