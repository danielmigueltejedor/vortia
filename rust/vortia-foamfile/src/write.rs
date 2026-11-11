//! Writing the case format: headers, dictionaries and values.
//!
//! The output is what OpenFOAM itself reads, laid out the way it writes it,
//! so a file can go back and forth between the two without diffs beyond
//! number formatting.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use crate::dict::{Dictionary, Entry, Value};
use crate::error::{FoamError, Result};

const BANNER: &str = "\
/*--------------------------------*- C++ -*----------------------------------*\\
  Vortia
\\*---------------------------------------------------------------------------*/
";
const RULE: &str =
    "// * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //\n";
const END: &str =
    "// ************************************************************************* //\n";

/// A number as it should appear in a file: shortest form that reads back to
/// the same `f64`, in exponent notation only when plain digits would be long.
pub fn scalar(v: f64) -> String {
    if !v.is_finite() {
        // OpenFOAM's own spelling, which it reads back.
        return if v.is_nan() {
            "nan".into()
        } else if v > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let a = v.abs();
    if a == 0.0 || (1e-4..1e7).contains(&a) {
        format!("{v}")
    } else {
        format!("{v:e}")
    }
}

/// The `FoamFile` header and the banner around it.
pub fn header(out: &mut String, class: &str, location: Option<&str>, object: &str) {
    out.push_str(BANNER);
    out.push_str("FoamFile\n{\n");
    let _ = writeln!(out, "    version     2.0;");
    let _ = writeln!(out, "    format      ascii;");
    let _ = writeln!(out, "    arch        \"LSB;label=32;scalar=64\";");
    let _ = writeln!(out, "    class       {class};");
    if let Some(loc) = location {
        let _ = writeln!(out, "    location    \"{loc}\";");
    }
    let _ = writeln!(out, "    object      {object};");
    out.push_str("}\n");
    out.push_str(RULE);
    out.push('\n');
}

pub fn footer(out: &mut String) {
    out.push('\n');
    out.push_str(END);
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes one value inline. Lists longer than `LONG` go one item per line.
pub fn value(out: &mut String, v: &Value, level: usize) {
    const LONG: usize = 10;
    match v {
        Value::Word(w) => out.push_str(w),
        Value::Str(s) => out.push_str(&quote(s)),
        Value::Label(l) => {
            let _ = write!(out, "{l}");
        }
        Value::Scalar(s) => out.push_str(&scalar(*s)),
        Value::Dimensions(items) => {
            out.push('[');
            join(out, items, level);
            out.push(']');
        }
        Value::List(items) if items.len() > LONG => {
            let _ = writeln!(out, "\n{}\n(", items.len());
            for item in items {
                value(out, item, level);
                out.push('\n');
            }
            out.push(')');
        }
        Value::List(items) => {
            if !items.is_empty() {
                let _ = write!(out, "{}", items.len());
            }
            out.push('(');
            join(out, items, level);
            out.push(')');
        }
        Value::Dict(d) => {
            out.push_str("{\n");
            dictionary(out, d, level + 1);
            indent(out, level);
            out.push('}');
        }
        Value::Keyed(name, d) => {
            out.push_str(name);
            out.push('\n');
            indent(out, level);
            out.push_str("{\n");
            dictionary(out, d, level + 1);
            indent(out, level);
            out.push('}');
        }
    }
}

fn join(out: &mut String, items: &[Value], level: usize) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        value(out, item, level);
    }
}

/// Writes the entries of a dictionary at `level`.
pub fn dictionary(out: &mut String, d: &Dictionary, level: usize) {
    for (key, entry) in d.entries() {
        indent(out, level);
        let key = if key.chars().any(|c| c.is_whitespace() || "|*\"".contains(c)) {
            quote(key)
        } else {
            key.to_owned()
        };
        match entry {
            Entry::Dict(sub) => {
                out.push_str(&key);
                out.push('\n');
                indent(out, level);
                out.push_str("{\n");
                dictionary(out, sub, level + 1);
                indent(out, level);
                out.push_str("}\n");
            }
            Entry::Values(values) => {
                let _ = write!(out, "{key:<15} ");
                join(out, values, level);
                out.push_str(";\n");
            }
            Entry::Directive(values) => {
                out.push_str(&key);
                out.push(' ');
                join(out, values, level);
                out.push('\n');
            }
        }
    }
}

/// Replaces `path` with `contents` atomically: a reader sees the old file or
/// the new one, never half of it.
pub fn atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let io = |source| FoamError::Io {
        path: path.to_owned(),
        source,
    };
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = std::fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(contents)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Tokenizer;

    #[test]
    fn scalars_read_back_exactly() {
        for v in [
            0.0,
            -0.0,
            1.0,
            101603.0,
            2.5e-3,
            1e-20,
            3.71235e-14,
            -172.252,
            1e300,
        ] {
            let s = scalar(v);
            assert_eq!(s.parse::<f64>().unwrap(), v, "{s}");
        }
        assert_eq!(scalar(101603.0), "101603");
        assert_eq!(scalar(1e-20), "1e-20");
    }

    #[test]
    fn a_dictionary_round_trips() {
        let src = "type totalPressure; p0 uniform 101325; gamma 1.4; \
                   sub { a (1 2 3); \"(front|back)\" { type wedge; } } \
                   long 12(1 2 3 4 5 6 7 8 9 10 11 12); dims [0 1 -1 0 0 0 0]; s \"a b\";";
        let d = Dictionary::parse(&mut Tokenizer::new(src.as_bytes()), false, 0).unwrap();
        let mut out = String::new();
        dictionary(&mut out, &d, 0);
        let back = Dictionary::parse(&mut Tokenizer::new(out.as_bytes()), false, 0).unwrap();
        assert_eq!(d, back, "{out}");
    }
}
