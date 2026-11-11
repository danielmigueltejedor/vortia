//! Dictionaries and the values inside them.
//!
//! A dictionary is an ordered list of entries. An entry is either a nested
//! dictionary (`key { ... }`) or a stream of values ending in `;`. Values
//! nest: `(...)` is a list, `[...]` a dimension set, `N(...)` a list that
//! declares its length, `N{v}` a list of N copies of one value.

use crate::error::{FoamError, Result};
use crate::token::{Token, TokenKind, Tokenizer};

/// How deep dictionaries and lists may nest before the input is refused.
pub const MAX_DEPTH: u32 = 64;

/// The longest uniform list (`N{v}`) expanded when a value is read.
const MAX_UNIFORM: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Word(String),
    Str(String),
    Label(i64),
    Scalar(f64),
    List(Vec<Value>),
    Dimensions(Vec<Value>),
    Dict(Dictionary),
    /// `name { ... }` inside a list, which counts as one item (the
    /// `boundary` file is a list of these).
    Keyed(String, Dictionary),
}

impl Value {
    /// How many values this holds once expanded, itself included, with text
    /// counted by the values its bytes would fill: what a uniform list of it
    /// costs per copy.
    fn weight(&self) -> usize {
        match self {
            Value::List(items) | Value::Dimensions(items) => items
                .iter()
                .fold(1usize, |acc, v| acc.saturating_add(v.weight())),
            Value::Dict(d) => d.weight(),
            Value::Keyed(k, d) => text_weight(k).saturating_add(d.weight()),
            Value::Word(w) | Value::Str(w) => text_weight(w),
            Value::Label(_) | Value::Scalar(_) => 1,
        }
    }

    pub fn as_word(&self) -> Option<&str> {
        match self {
            Value::Word(w) | Value::Str(w) => Some(w),
            _ => None,
        }
    }

    pub fn as_scalar(&self) -> Option<f64> {
        match *self {
            Value::Scalar(v) => Some(v),
            Value::Label(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn as_label(&self) -> Option<i64> {
        match *self {
            Value::Label(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Dict(Dictionary),
    /// The values between the key and its `;`.
    Values(Vec<Value>),
    /// A `#include`, `#calc`, etc. Kept, not evaluated.
    Directive(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dictionary {
    entries: Vec<(String, Entry)>,
}

/// Text costs one value plus the values its bytes would fill.
fn text_weight(text: &str) -> usize {
    1 + text.len() / std::mem::size_of::<Value>()
}

impl Dictionary {
    fn weight(&self) -> usize {
        self.entries.iter().fold(1usize, |acc, (k, e)| {
            let w = text_weight(k).saturating_add(match e {
                Entry::Dict(d) => d.weight(),
                Entry::Values(vs) | Entry::Directive(vs) => {
                    vs.iter().fold(1usize, |a, v| a.saturating_add(v.weight()))
                }
            });
            acc.saturating_add(w)
        })
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), e))
    }

    /// The last entry with this key: a later entry overrides an earlier one.
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, e)| e)
    }

    pub fn insert(&mut self, key: impl Into<String>, entry: Entry) {
        self.entries.push((key.into(), entry));
    }

    pub fn dict(&self, key: &str) -> Option<&Dictionary> {
        match self.get(key)? {
            Entry::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn values(&self, key: &str) -> Option<&[Value]> {
        match self.get(key)? {
            Entry::Values(v) => Some(v),
            _ => None,
        }
    }

    /// The single value of `key;`-style entries such as `type wall;`.
    pub fn value(&self, key: &str) -> Option<&Value> {
        match self.values(key)? {
            [v] => Some(v),
            _ => None,
        }
    }

    pub fn word(&self, key: &str) -> Option<&str> {
        self.value(key)?.as_word()
    }

    pub fn scalar(&self, key: &str) -> Option<f64> {
        self.value(key)?.as_scalar()
    }

    pub fn label(&self, key: &str) -> Option<i64> {
        self.value(key)?.as_label()
    }

    /// Reads entries until a closing `}` (when `nested`) or the end of input.
    pub fn parse(tok: &mut Tokenizer<'_>, nested: bool, depth: u32) -> Result<Self> {
        if depth > MAX_DEPTH {
            return Err(FoamError::parse(tok.line(), "dictionaries nest too deeply"));
        }
        let mut dict = Dictionary::new();
        loop {
            let Some(t) = tok.next()? else {
                if nested {
                    return Err(FoamError::parse(tok.line(), "dictionary is never closed"));
                }
                return Ok(dict);
            };
            let key = match t.kind {
                TokenKind::Punct(b'}') if nested => return Ok(dict),
                TokenKind::Punct(b';') => continue,
                TokenKind::Word(w) | TokenKind::Str(w) => w,
                // Numeric keys occur, e.g. in tabulated thermophysical data.
                TokenKind::Label(v) => v.to_string(),
                TokenKind::Scalar(v) => v.to_string(),
                TokenKind::Punct(_) => {
                    return Err(FoamError::parse(
                        t.line,
                        format!("expected a keyword, found {}", t.describe()),
                    ));
                }
            };
            if key.starts_with('#') {
                dict.insert(key, Entry::Directive(read_directive(tok, depth)?));
                continue;
            }
            if tok.peek()?.is_some_and(|t| t.is_punct(b'{')) {
                tok.expect_punct(b'{')?;
                let sub = Dictionary::parse(tok, true, depth + 1)?;
                dict.insert(key, Entry::Dict(sub));
            } else {
                let mut values = Vec::new();
                loop {
                    let t = tok.expect_any(&format!("';' after '{key}'"))?;
                    if t.is_punct(b';') {
                        break;
                    }
                    values.push(parse_value(t, tok, depth + 1)?);
                }
                dict.insert(key, Entry::Values(values));
            }
        }
    }
}

/// Directives take the rest of their line's values up to `;` or one value.
fn read_directive(tok: &mut Tokenizer<'_>, depth: u32) -> Result<Vec<Value>> {
    let line = tok.line();
    let mut out = Vec::new();
    while let Some(t) = tok.peek()? {
        if t.line != line || t.is_punct(b'}') {
            break;
        }
        let t = tok.expect_any("a directive argument")?;
        if t.is_punct(b';') {
            break;
        }
        out.push(parse_value(t, tok, depth + 1)?);
    }
    Ok(out)
}

/// Parses one value, of which `first` is the first token.
pub fn parse_value(first: Token, tok: &mut Tokenizer<'_>, depth: u32) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(FoamError::parse(first.line, "values nest too deeply"));
    }
    match first.kind {
        TokenKind::Word(w) => Ok(Value::Word(w)),
        TokenKind::Str(s) => Ok(Value::Str(s)),
        TokenKind::Scalar(v) => Ok(Value::Scalar(v)),
        TokenKind::Label(n) => {
            let next_is = |tok: &mut Tokenizer<'_>, c| -> Result<bool> {
                Ok(tok.peek()?.is_some_and(|t: &Token| t.is_punct(c)))
            };
            if next_is(tok, b'(')? {
                let open = tok.expect_any("'('")?;
                let items = parse_list(open.line, tok, depth)?;
                if usize::try_from(n).ok() != Some(items.len()) {
                    return Err(FoamError::parse(
                        first.line,
                        format!("list declares {n} items but holds {}", items.len()),
                    ));
                }
                Ok(Value::List(items))
            } else if next_is(tok, b'{')? {
                tok.expect_punct(b'{')?;
                let count = usize::try_from(n)
                    .ok()
                    .filter(|&c| c <= MAX_UNIFORM)
                    .ok_or_else(|| {
                        FoamError::parse(first.line, format!("uniform list of {n} items"))
                    })?;
                let t = tok.expect_any("a value")?;
                let v = parse_value(t, tok, depth + 1)?;
                tok.expect_punct(b'}')?;
                // The budget is on the expanded size: `N{M{0}}` holds N×M values.
                if count.saturating_mul(v.weight()) > MAX_UNIFORM {
                    return Err(FoamError::parse(
                        first.line,
                        format!("uniform list of {n} items expands too far"),
                    ));
                }
                Ok(Value::List(vec![v; count]))
            } else {
                Ok(Value::Label(n))
            }
        }
        TokenKind::Punct(b'(') => Ok(Value::List(parse_list(first.line, tok, depth)?)),
        TokenKind::Punct(b'[') => {
            let mut items = Vec::new();
            loop {
                let t = tok.expect_any("']'")?;
                if t.is_punct(b']') {
                    return Ok(Value::Dimensions(items));
                }
                items.push(parse_value(t, tok, depth + 1)?);
            }
        }
        TokenKind::Punct(b'{') => Ok(Value::Dict(Dictionary::parse(tok, true, depth + 1)?)),
        TokenKind::Punct(c) => Err(FoamError::parse(
            first.line,
            format!("unexpected '{}'", c as char),
        )),
    }
}

fn parse_list(line: u32, tok: &mut Tokenizer<'_>, depth: u32) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    loop {
        let Some(t) = tok.next()? else {
            return Err(FoamError::parse(line, "list is never closed"));
        };
        if t.is_punct(b')') {
            return Ok(items);
        }
        if let TokenKind::Word(name) = &t.kind
            && tok.peek()?.is_some_and(|n| n.is_punct(b'{'))
        {
            tok.expect_punct(b'{')?;
            let d = Dictionary::parse(tok, true, depth + 1)?;
            items.push(Value::Keyed(name.clone(), d));
            continue;
        }
        items.push(parse_value(t, tok, depth + 1)?);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Dictionary {
        Dictionary::parse(&mut Tokenizer::new(src.as_bytes()), false, 0).unwrap()
    }

    #[test]
    fn reads_nested_dictionaries_and_values() {
        let d = parse(
            "application simpleFoam; endTime 500;\n\
             PISO { nCorrectors 2; } divSchemes { div(phi,U) bounded Gauss linearUpwind grad(U); }",
        );
        assert_eq!(d.word("application"), Some("simpleFoam"));
        assert_eq!(d.label("endTime"), Some(500));
        assert_eq!(d.dict("PISO").and_then(|p| p.label("nCorrectors")), Some(2));
        let div = d
            .dict("divSchemes")
            .and_then(|s| s.values("div(phi,U)"))
            .unwrap();
        assert_eq!(div.len(), 4);
    }

    #[test]
    fn reads_counted_uniform_and_dimension_values() {
        let d = parse("a 2(1 2); b 3{0.5}; c [0 1 -1 0 0 0 0]; u uniform (0 0 1);");
        assert_eq!(
            d.value("a").and_then(Value::as_list).map(<[_]>::len),
            Some(2)
        );
        assert_eq!(
            d.value("b"),
            Some(&Value::List(vec![Value::Scalar(0.5); 3]))
        );
        assert!(matches!(d.value("c"), Some(Value::Dimensions(v)) if v.len() == 7));
        assert_eq!(d.values("u").map(<[_]>::len), Some(2));
    }

    #[test]
    fn a_comment_after_a_number_keeps_it_a_number() {
        assert_eq!(parse("endTime 1/*comment*/;").label("endTime"), Some(1));
    }

    #[test]
    fn a_later_entry_overrides_an_earlier_one() {
        assert_eq!(parse("a 1; a 2;").label("a"), Some(2));
    }

    #[test]
    fn directives_are_kept_without_a_semicolon() {
        let d = parse("#include \"initialConditions\"\nendTime 1;");
        assert!(matches!(d.get("#include"), Some(Entry::Directive(v)) if v.len() == 1));
        assert_eq!(d.label("endTime"), Some(1));
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        for src in [
            "a (1 2",
            "a { b 1;",
            "a 3(1 2);",
            "a 99999999999{1};",
            "} x",
            "a [1 2",
            "a 1048576{1048576{0}};",
            "a 2{1048576{0}};",
            "a 1048576{(1 2)};",
        ] {
            assert!(
                Dictionary::parse(&mut Tokenizer::new(src.as_bytes()), false, 0).is_err(),
                "{src}"
            );
        }
        // Text counts by its bytes: a million copies of a kilobyte is a
        // gigabyte however few values it is.
        let big = format!("a 1000{{1000{{\"{}\"}}}};", "x".repeat(1024));
        assert!(Dictionary::parse(&mut Tokenizer::new(big.as_bytes()), false, 0).is_err());
        let keyed = |k: &str| format!("a 100{{100{{{{{k} 1;}}}}}};");
        assert!(Dictionary::parse(&mut Tokenizer::new(keyed("k").as_bytes()), false, 0).is_ok());
        let long = keyed(&"k".repeat(16 * 1024));
        assert!(Dictionary::parse(&mut Tokenizer::new(long.as_bytes()), false, 0).is_err());
        let deep = "(".repeat(10_000);
        assert!(
            Dictionary::parse(
                &mut Tokenizer::new(format!("a {deep}").as_bytes()),
                false,
                0
            )
            .is_err()
        );
    }
}
