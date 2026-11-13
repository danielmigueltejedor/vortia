//! The numbers of `system/fvSolution` (tolerances, relaxation factors, residual targets, the
//! number of correctors…) and the scheme words of `system/fvSchemes`, read for the Numerics and
//! Schemes tabs and edited in place. In fvSolution the names (solver, smoother, preconditioner)
//! are shown but not edited; in fvSchemes a scheme is edited as the words it is written with.

use std::path::Path;

use vortia_foamfile::dict::{Dictionary, Entry, Value};
use vortia_foamfile::file::FoamFile;
use vortia_foamfile::write;

use crate::conditions::{Item, checked, set_at};

/// The two dictionaries of `system` that are edited here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum File {
    Solution,
    Schemes,
}

impl File {
    pub fn name(self) -> &'static str {
        match self {
            File::Solution => "fvSolution",
            File::Schemes => "fvSchemes",
        }
    }
}

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

/// A scheme is edited as words: a name, a number, or a reference such as `grad(U)`.
fn scheme_words(key: &str, values: &[Value]) -> bool {
    !key.starts_with('$')
        && !values.is_empty()
        && values
            .iter()
            .all(|v| matches!(v, Value::Word(_) | Value::Scalar(_) | Value::Label(_)))
}

fn collect(
    dict: &Dictionary,
    file: File,
    path: &mut Vec<String>,
    groups: &mut Vec<Group>,
    count: &mut usize,
) {
    let mut items = Vec::new();
    for (key, entry) in dict.entries() {
        if let Entry::Values(vs) = entry {
            *count += 1;
            if *count <= MAX_ITEMS {
                let mut item = Item::new(key, vs);
                if file == File::Schemes {
                    item.editable = scheme_words(key, vs);
                }
                items.push(item);
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
            collect(inner, file, path, groups, count);
            path.pop();
        }
    }
}

pub fn read(case: &Path, file: File) -> Result<Vec<Group>, String> {
    let (_, dict) = FoamFile::open(case.join("system").join(file.name()))
        .and_then(|f| f.dictionary())
        .map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    collect(&dict, file, &mut Vec::new(), &mut groups, &mut 0);
    Ok(groups)
}

/// What to write for an edited scheme: its words, one space apart, and nothing that could end
/// the entry or start a comment.
fn checked_scheme(item: &Item) -> Result<String, String> {
    let words: Vec<&str> = item.text.split_whitespace().collect();
    let allowed = |c: char| c.is_alphanumeric() || "_.()|*+-$:<>,".contains(c);
    if words.is_empty() {
        return Err(format!("{} needs a scheme.", item.key));
    }
    if let Some(bad) = words.iter().flat_map(|w| w.chars()).find(|c| !allowed(*c)) {
        return Err(format!("{}: '{bad}' cannot be part of a scheme.", item.key));
    }
    Ok(words.join(" "))
}

/// The text of the file with every edited item of `groups` applied.
pub fn apply(text: &str, groups: &[Group], file: File) -> Result<String, String> {
    let mut text = text.to_owned();
    for group in groups {
        let path: Vec<&str> = group.path.iter().map(String::as_str).collect();
        for item in group.items.iter().filter(|i| i.editable && i.changed()) {
            let value = match file {
                File::Solution => checked(item)?,
                File::Schemes => checked_scheme(item)?,
            };
            text = set_at(&text, &path, &item.key, &value)?;
        }
    }
    Ok(text)
}

/// Writes the edits into the case's file, atomically.
pub fn save(case: &Path, file: File, groups: &[Group]) -> Result<(), String> {
    let path = case.join("system").join(file.name());
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let edited = apply(&text, groups, file)?;
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
        let groups = read(case.path(), File::Solution).unwrap();
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
        let mut groups = read(case.path(), File::Solution).unwrap();
        // `p` is in three dictionaries; only the relaxation factor is edited.
        groups[4].items[0].text = "0.2".into();
        groups[0].items[2].text = "0.05".into(); // relTol of solvers/p
        save(case.path(), File::Solution, &groups).unwrap();
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
        let mut groups = read(case.path(), File::Solution).unwrap();
        for bad in ["fast", "", "1e", "inf", "(1 2 3)"] {
            groups[0].items[1].text = bad.into();
            assert!(save(case.path(), File::Solution, &groups).is_err(), "{bad}");
        }
        let text = std::fs::read_to_string(case.path().join("system/fvSolution")).unwrap();
        assert_eq!(text, SOLUTION);
    }

    #[test]
    fn a_missing_or_broken_file_is_an_error() {
        let empty = tempfile::tempdir().unwrap();
        assert!(read(empty.path(), File::Solution).is_err());
        std::fs::create_dir_all(empty.path().join("system")).unwrap();
        std::fs::write(empty.path().join("system/fvSolution"), "{{{ not").unwrap();
        assert!(read(empty.path(), File::Solution).is_err());
    }

    const SCHEMES: &str = "\
FoamFile
{
    version     2.0;
    format      ascii;
    class       dictionary;
    object      fvSchemes;
}
gradSchemes
{
    default         Gauss linear;
    limited         cellLimited Gauss linear 1;
    grad(U)         $limited;
}
divSchemes
{
    default         none;
    div(phi,U)      bounded Gauss linearUpwind grad(U);
    div(((rho*nuEff)*dev2(T(grad(U)))))    Gauss linear;
}
";

    fn schemes_case() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("system")).unwrap();
        std::fs::write(dir.path().join("system/fvSchemes"), SCHEMES).unwrap();
        dir
    }

    #[test]
    fn the_schemes_are_listed_and_edited_as_words() {
        let case = schemes_case();
        let mut groups = read(case.path(), File::Schemes).unwrap();
        assert_eq!(groups[0].title(), "gradSchemes");
        let item = |g: &Group, key: &str| g.items.iter().find(|i| i.key == key).unwrap().clone();
        assert_eq!(
            item(&groups[1], "div(phi,U)").text,
            "bounded Gauss linearUpwind grad(U)"
        );
        assert!(item(&groups[1], "div(phi,U)").editable);
        let long = "div(((rho*nuEff)*dev2(T(grad(U)))))";
        assert!(item(&groups[1], long).editable);
        for g in &mut groups {
            for i in &mut g.items {
                if i.key == "div(phi,U)" {
                    i.text = "  bounded Gauss   upwind ".into();
                }
                if i.key == "default" && g.path == ["gradSchemes"] {
                    i.text = "Gauss linear".into();
                }
            }
        }
        save(case.path(), File::Schemes, &groups).unwrap();
        let text = std::fs::read_to_string(case.path().join("system/fvSchemes")).unwrap();
        assert_eq!(
            text,
            SCHEMES.replace(
                "bounded Gauss linearUpwind grad(U);",
                "bounded Gauss upwind;"
            )
        );
    }

    #[test]
    fn a_scheme_that_could_break_the_file_is_refused() {
        let case = schemes_case();
        let mut groups = read(case.path(), File::Schemes).unwrap();
        for bad in [
            "",
            "   ",
            "Gauss; evil 1",
            "a } b",
            "x // y",
            "\"q\"",
            "#include z",
        ] {
            groups[1].items[1].text = bad.into();
            assert!(
                save(case.path(), File::Schemes, &groups).is_err(),
                "{bad:?}"
            );
        }
        let text = std::fs::read_to_string(case.path().join("system/fvSchemes")).unwrap();
        assert_eq!(text, SCHEMES);
    }
}
