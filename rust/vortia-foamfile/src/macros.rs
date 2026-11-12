//! `$name` macros, which a dictionary uses to share entries.
//!
//! Two forms occur in case files: `div(phi,e) $energy;` takes the values of
//! the entry `energy`, and `$p;` inside a dictionary merges the entries of the
//! dictionary `p` into it, entries that follow overriding them (`pFinal { $p;
//! relTol 0; }`). A name is looked up in the dictionary that uses it, then in
//! each dictionary around it; `$:name` starts from the top, and `a.b` goes
//! into the dictionary `a`.
//!
//! Derived from the behaviour of OpenFOAM's dictionary macro expansion
//! (OpenFOAM, GPL-3.0-or-later). Hostile input is bounded: macros may nest only
//! so deep, and the values they expand to are counted.

use crate::dict::{Dictionary, Entry, Value};
use crate::error::{FoamError, Result};

/// How many macros may stand inside one another.
const MAX_NESTING: u32 = 16;
/// The most values and entries all the macros of one file may expand to.
const MAX_EXPANDED: usize = 1 << 20;

/// Whether a word is a macro: `$name`, `${name}`.
fn macro_name(word: &str) -> Option<&str> {
    let name = word.strip_prefix('$')?;
    let name = name
        .strip_prefix('{')
        .and_then(|n| n.strip_suffix('}'))
        .unwrap_or(name);
    (!name.is_empty()).then_some(name)
}

struct Expander {
    budget: usize,
}

impl Dictionary {
    /// This dictionary with every `$name` replaced by what it names.
    pub fn expand_macros(&self) -> Result<Self> {
        let mut e = Expander {
            budget: MAX_EXPANDED,
        };
        e.dictionary(self, &mut Vec::new(), 0)
    }
}

impl Expander {
    fn spend(&mut self, n: usize) -> Result<()> {
        self.budget = self
            .budget
            .checked_sub(n)
            .ok_or_else(|| FoamError::Invalid("macros expand to too much".into()))?;
        Ok(())
    }

    /// Looks `name` up in the innermost dictionary of `scopes`, then in each
    /// one around it.
    fn find<'a>(name: &str, scopes: &[&'a Dictionary]) -> Option<&'a Entry> {
        let (from_top, path) = match name.strip_prefix(':') {
            Some(rest) => (true, rest),
            None => (false, name),
        };
        let mut parts = path.split('.');
        let first = parts.next()?;
        let mut entry = if from_top {
            scopes.first()?.get(first)?
        } else {
            scopes.iter().rev().find_map(|d| d.get(first))?
        };
        for part in parts {
            entry = match entry {
                Entry::Dict(d) => d.get(part)?,
                _ => return None,
            };
        }
        Some(entry)
    }

    /// `src` with its macros expanded; `scopes` are the dictionaries around
    /// it, outermost first.
    fn dictionary<'a>(
        &mut self,
        src: &'a Dictionary,
        scopes: &mut Vec<&'a Dictionary>,
        depth: u32,
    ) -> Result<Dictionary> {
        if depth > MAX_NESTING {
            return Err(FoamError::Invalid("macros nest too deeply".into()));
        }
        scopes.push(src);
        let result = self.entries(src, scopes, depth);
        scopes.pop();
        result
    }

    fn entries<'a>(
        &mut self,
        src: &'a Dictionary,
        scopes: &mut Vec<&'a Dictionary>,
        depth: u32,
    ) -> Result<Dictionary> {
        let mut out = Dictionary::new();
        for (key, entry) in src.entries() {
            self.spend(1)?;
            match (macro_name(key), entry) {
                (Some(name), Entry::Values(values)) if values.is_empty() => {
                    let Some(Entry::Dict(merged)) = Self::find(name, scopes) else {
                        return Err(FoamError::Invalid(format!(
                            "'{key}' does not name a dictionary"
                        )));
                    };
                    let expanded = self.dictionary(merged, scopes, depth + 1)?;
                    for (k, e) in expanded.entries() {
                        out.set(k, e.clone());
                    }
                }
                (_, Entry::Dict(d)) => {
                    out.insert(key, Entry::Dict(self.dictionary(d, scopes, depth + 1)?));
                }
                (_, Entry::Values(values)) => {
                    out.insert(key, Entry::Values(self.values(values, scopes, depth)?));
                }
                (_, Entry::Directive(_)) => out.insert(key, entry.clone()),
            }
        }
        Ok(out)
    }

    fn values<'a>(
        &mut self,
        values: &'a [Value],
        scopes: &mut Vec<&'a Dictionary>,
        depth: u32,
    ) -> Result<Vec<Value>> {
        if depth > MAX_NESTING {
            return Err(FoamError::Invalid("macros nest too deeply".into()));
        }
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            self.spend(1)?;
            match value {
                Value::Word(w) => match macro_name(w) {
                    Some(name) => match Self::find(name, scopes) {
                        Some(Entry::Values(vs)) => {
                            out.extend(self.values(vs, scopes, depth + 1)?);
                        }
                        Some(Entry::Dict(d)) => {
                            out.push(Value::Dict(self.dictionary(d, scopes, depth + 1)?));
                        }
                        _ => return Err(FoamError::Invalid(format!("'{w}' names nothing"))),
                    },
                    None => out.push(value.clone()),
                },
                Value::List(items) => {
                    out.push(Value::List(self.values(items, scopes, depth)?));
                }
                Value::Dimensions(items) => {
                    out.push(Value::Dimensions(self.values(items, scopes, depth)?));
                }
                Value::Dict(d) => {
                    out.push(Value::Dict(self.dictionary(d, scopes, depth + 1)?));
                }
                Value::Keyed(k, d) => {
                    out.push(Value::Keyed(
                        k.clone(),
                        self.dictionary(d, scopes, depth + 1)?,
                    ));
                }
                other => out.push(other.clone()),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::dict::{Dictionary, Value};
    use crate::token::Tokenizer;

    fn parse(src: &str) -> Dictionary {
        Dictionary::parse(&mut Tokenizer::new(src.as_bytes()), false, 0).unwrap()
    }

    fn words(d: &Dictionary, key: &str) -> Vec<String> {
        d.values(key)
            .unwrap()
            .iter()
            .filter_map(|v| v.as_word().map(str::to_owned))
            .collect()
    }

    #[test]
    fn a_value_macro_takes_the_values_it_names() {
        let d = parse(
            "gradSchemes { limited cellLimited Gauss linear 1; grad(U) $limited; } \
             divSchemes { energy bounded Gauss linearUpwind grad(U); div(phi,e) $energy; \
             div(phi,h) $energy; }",
        )
        .expand_macros()
        .unwrap();
        let grad = d.dict("gradSchemes").unwrap();
        assert_eq!(
            grad.values("grad(U)")
                .unwrap()
                .iter()
                .filter_map(Value::as_word)
                .collect::<Vec<_>>(),
            ["cellLimited", "Gauss", "linear"]
        );
        assert_eq!(
            grad.values("grad(U)").unwrap().last().unwrap().as_scalar(),
            Some(1.0)
        );
        let div = d.dict("divSchemes").unwrap();
        assert_eq!(
            words(div, "div(phi,h)"),
            ["bounded", "Gauss", "linearUpwind", "grad(U)"]
        );
    }

    #[test]
    fn a_dictionary_macro_merges_and_later_entries_win() {
        let d = parse(
            "solvers { p { solver PCG; relTol 0.01; tolerance 1e-7; } \
             pFinal { $p; relTol 0; } }",
        )
        .expand_macros()
        .unwrap();
        let fin = d.dict("solvers").unwrap().dict("pFinal").unwrap();
        assert_eq!(fin.word("solver"), Some("PCG"));
        assert_eq!(fin.scalar("relTol"), Some(0.0));
        assert_eq!(fin.scalar("tolerance"), Some(1e-7));
    }

    #[test]
    fn names_are_found_in_enclosing_dictionaries_and_from_the_top() {
        let d = parse("a 5; inner { a 7; x $a; y $:a; z $inner2.q; } inner2 { q 9; }")
            .expand_macros()
            .unwrap();
        let inner = d.dict("inner").unwrap();
        assert_eq!(inner.scalar("x"), Some(7.0));
        assert_eq!(inner.scalar("y"), Some(5.0));
        assert_eq!(inner.scalar("z"), Some(9.0));
    }

    #[test]
    fn hostile_macros_are_refused_not_followed() {
        // A name that is nowhere, a macro that names itself, and one that
        // doubles at every step.
        assert!(parse("x $nowhere;").expand_macros().is_err());
        assert!(parse("x $x;").expand_macros().is_err());
        let mut src = String::from("a0 1;");
        for i in 1..40 {
            src.push_str(&format!("a{i} $a{} $a{};", i - 1, i - 1));
        }
        assert!(parse(&src).expand_macros().is_err());
        assert!(parse("d { $d; }").expand_macros().is_err());
    }
}
