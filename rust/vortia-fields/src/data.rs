//! `uniform v` and `nonuniform List<T> N(...)`: the two ways a field's values
//! are written.

use std::fmt::Write as _;

use vortia_foamfile::Value;

use crate::error::{FieldError, Result};
use crate::value::{FieldValue, format};

#[derive(Debug, Clone, PartialEq)]
pub enum Data<T> {
    Uniform(T),
    NonUniform(Vec<T>),
}

impl<T: FieldValue> Data<T> {
    /// Parses the values of an `internalField` or patch `value` entry.
    pub fn parse(values: &[Value], what: &str) -> Result<Self> {
        let bad = |why: &str| FieldError::Invalid(format!("{what}: {why}"));
        match values {
            [Value::Word(w), v] if w == "uniform" => T::from_value(v)
                .map(Data::Uniform)
                .ok_or_else(|| bad(&format!("not a {}", T::NAME))),
            [Value::Word(w), Value::Word(list), items] if w == "nonuniform" => {
                let expected = format!("List<{}>", T::NAME);
                if *list != expected {
                    return Err(bad(&format!("holds {list}, expected {expected}")));
                }
                let items = items
                    .as_list()
                    .ok_or_else(|| bad("the values are not a list"))?;
                items
                    .iter()
                    .map(T::from_value)
                    .collect::<Option<Vec<_>>>()
                    .map(Data::NonUniform)
                    .ok_or_else(|| bad(&format!("an item is not a {}", T::NAME)))
            }
            // An empty patch's value: `nonuniform 0()`.
            [Value::Word(w), Value::List(items)] if w == "nonuniform" && items.is_empty() => {
                Ok(Data::NonUniform(Vec::new()))
            }
            _ => Err(bad(
                "expected 'uniform VALUE' or 'nonuniform List<T> N(...)'",
            )),
        }
    }

    /// The values for `n` items; `None` when a non-uniform list has another length.
    pub fn expand(&self, n: usize) -> Option<Vec<T>> {
        match self {
            Data::Uniform(v) => Some(vec![*v; n]),
            Data::NonUniform(items) => (items.len() == n).then(|| items.clone()),
        }
    }

    /// How many values a non-uniform list holds; `None` for a uniform value.
    pub fn count(&self) -> Option<usize> {
        match self {
            Data::Uniform(_) => None,
            Data::NonUniform(items) => Some(items.len()),
        }
    }

    /// Writes `uniform v` or `nonuniform List<T> N(...)`.
    pub fn write(&self, out: &mut String) {
        match self {
            Data::Uniform(v) => {
                let _ = write!(out, "uniform {}", format(*v));
            }
            Data::NonUniform(items) => {
                let _ = write!(out, "nonuniform List<{}> \n{}\n(\n", T::NAME, items.len());
                for &v in items {
                    out.push_str(&format(v));
                    out.push('\n');
                }
                out.push_str(")\n");
            }
        }
    }
}
