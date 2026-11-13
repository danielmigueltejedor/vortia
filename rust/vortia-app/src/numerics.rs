//! The numbers of `system/fvSolution` (tolerances, relaxation factors, residual targets, the
//! number of correctors…), read for the Numerics tab and edited in place. Names such as the
//! solver, the smoother or the preconditioner are shown but not edited.

use std::path::Path;

use vortia_foamfile::dict::{Dictionary, Entry};
use vortia_foamfile::file::FoamFile;
use vortia_foamfile::write;

use crate::conditions::{Item, checked, set_at};

const MAX_DEPTH: usize = 6;
const MAX_ITEMS: usize = 500;

/// The entries of one dictionary of the file, such as `solvers/p` or `relaxationFactors/fields`.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub path: Vec<String>,
    pub items: Vec<Item>,
}

impl Group {
    pub fn title(&self) -> String {
        self.path.join(" / ")
    }
}

fn collect(dict: &Dictionary, path: &mut Vec<String>, groups: &mut Vec<Group>, count: &mut usize) {
    let mut items = Vec::new();
    for (key, entry) in dict.entries() {
        if let Entry::Values(vs) = entry {
            *count += 1;
            if *count <= MAX_ITEMS {
                items.push(Item::new(key, vs));
            }
        }
    }
    if !items.is_empty() {
        groups.push(Group {
            path: path.clone(),
            items,
        });
    }
    if path.len() >= MAX_DEPTH {
        return;
    }
    for (key, entry) in dict.entries() {
        if let Entry::Dict(inner) = entry
            && *count <= MAX_ITEMS
        {
            path.push(key.to_owned());
            collect(inner, path, groups, count);
            path.pop();
        }
    }
}

pub fn read(case: &Path) -> Result<Vec<Group>, String> {
    let (_, dict) = FoamFile::open(case.join("system").join("fvSolution"))
        .and_then(|f| f.dictionary())
        .map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    collect(&dict, &mut Vec::new(), &mut groups, &mut 0);
    Ok(groups)
}

/// The text of the file with every edited item of `groups` applied.
pub fn apply(text: &str, groups: &[Group]) -> Result<String, String> {
    let mut text = text.to_owned();
    for group in groups {
        let path: Vec<&str> = group.path.iter().map(String::as_str).collect();
        for item in group.items.iter().filter(|i| i.editable && i.changed()) {
            text = set_at(&text, &path, &item.key, &checked(item)?)?;
        }
    }
    Ok(text)
}

/// Writes the edits into the case's fvSolution, atomically.
pub fn save(case: &Path, groups: &[Group]) -> Result<(), String> {
    let path = case.join("system").join("fvSolution");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let edited = apply(&text, groups)?;
    write::atomic(&path, edited.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLUTION: &str = "\
FoamFile
{
    version     2.0;
    format      ascii;
    class       dictionary;
    object      fvSolution;
}
solvers
{
    p
    {
        solver          PCG;
        tolerance       1e-07;
        relTol          0.01;
    }
    pFinal
    {
        $p;
        relTol          0;
    }
}
SIMPLE
{
    nNonOrthogonalCorrectors 0;
    consistent      yes;
    residualControl
    {
        p               1e-04;
    }
}
relaxationFactors
{
    fields
    {
        p               0.3;
    }
}
";

    fn case() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("system")).unwrap();
        std::fs::write(dir.path().join("system/fvSolution"), SOLUTION).unwrap();
        dir
    }

    #[test]
    fn the_numbers_of_every_dictionary_are_listed() {
        let case = case();
        let groups = read(case.path()).unwrap();
        let titles: Vec<String> = groups.iter().map(Group::title).collect();
        assert_eq!(
            titles,
            [
                "solvers / p",
                "solvers / pFinal",
                "SIMPLE",
                "SIMPLE / residualControl",
                "relaxationFactors / fields"
            ]
        );
        let editable =
            |g: &Group, key: &str| g.items.iter().find(|i| i.key == key).unwrap().editable;
        assert!(editable(&groups[0], "tolerance"));
        assert!(!editable(&groups[0], "solver"));
        assert!(!editable(&groups[2], "consistent"));
        assert!(editable(&groups[2], "nNonOrthogonalCorrectors"));
    }

    #[test]
    fn an_edit_changes_one_number_and_nothing_else() {
        let case = case();
        let mut groups = read(case.path()).unwrap();
        // `p` is in three dictionaries; only the relaxation factor is edited.
        groups[4].items[0].text = "0.2".into();
        groups[0].items[2].text = "0.05".into(); // relTol of solvers/p
        save(case.path(), &groups).unwrap();
        let text = std::fs::read_to_string(case.path().join("system/fvSolution")).unwrap();
        assert_eq!(
            text,
            SOLUTION
                .replace("p               0.3;", "p               0.2;")
                .replace("relTol          0.01;", "relTol          0.05;")
        );
    }

    #[test]
    fn a_number_that_is_not_one_is_refused() {
        let case = case();
        let mut groups = read(case.path()).unwrap();
        for bad in ["fast", "", "1e", "inf", "(1 2 3)"] {
            groups[0].items[1].text = bad.into();
            assert!(save(case.path(), &groups).is_err(), "{bad}");
        }
        let text = std::fs::read_to_string(case.path().join("system/fvSolution")).unwrap();
        assert_eq!(text, SOLUTION);
    }

    #[test]
    fn a_missing_or_broken_file_is_an_error() {
        let empty = tempfile::tempdir().unwrap();
        assert!(read(empty.path()).is_err());
        std::fs::create_dir_all(empty.path().join("system")).unwrap();
        std::fs::write(empty.path().join("system/fvSolution"), "{{{ not").unwrap();
        assert!(read(empty.path()).is_err());
    }
}
