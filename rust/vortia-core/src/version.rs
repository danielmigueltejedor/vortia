//! Version strings of external tools, such as OpenFOAM (`v2606`, `14`, `9.4.2`).

use crate::error::{ErrorCode, Result, VortiaError};

/// A parsed version. Missing components are zero; `raw` keeps the trimmed original text.
/// Ordering compares major, minor and patch, then the raw text, as the C++ type does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Version {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub raw: String,
}

impl Version {
    /// Parses up to three dot-separated numeric components. A leading `v` is accepted, and
    /// anything after the digits of a component (`2412-patch1`) is ignored.
    pub fn parse(text: &str) -> Result<Self> {
        let trimmed = text.trim_matches([' ', '\t', '\r', '\n']);
        if trimmed.is_empty() {
            return Err(VortiaError::new(
                ErrorCode::ParseError,
                "empty version string",
                "",
            ));
        }
        let mut work = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
        let mut fields = [0i32; 3];
        for field in &mut fields {
            if work.is_empty() {
                break;
            }
            let (token, rest) = match work.split_once('.') {
                Some((token, rest)) => (token, Some(rest)),
                None => (work, None),
            };
            *field = numeric_prefix(token, trimmed)?;
            match rest {
                Some(rest) => work = rest,
                None => break,
            }
        }
        Ok(Self {
            major: fields[0],
            minor: fields[1],
            patch: fields[2],
            raw: trimmed.to_owned(),
        })
    }
}

fn numeric_prefix(token: &str, whole: &str) -> Result<i32> {
    let digits = token.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return Err(VortiaError::new(
            ErrorCode::ParseError,
            "version component is not numeric",
            whole,
        ));
    }
    token[..digits].parse().map_err(|_| {
        VortiaError::new(
            ErrorCode::ParseError,
            "version component out of range",
            whole,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_esi_style() {
        let v = Version::parse("v2606").expect("valid");
        assert_eq!((v.major, v.minor, v.raw.as_str()), (2606, 0, "v2606"));
    }

    #[test]
    fn parses_foundation_style() {
        assert_eq!(Version::parse("14").expect("valid").major, 14);
    }

    #[test]
    fn parses_dotted() {
        let v = Version::parse("9.4.2").expect("valid");
        assert_eq!((v.major, v.minor, v.patch), (9, 4, 2));
    }

    #[test]
    fn tolerates_whitespace_and_suffix() {
        assert_eq!(
            Version::parse("  v2412-patch1 ").expect("valid").major,
            2412
        );
    }

    #[test]
    fn rejects_garbage() {
        for text in ["", "abc", ".1"] {
            assert_eq!(
                Version::parse(text).expect_err("invalid").code,
                ErrorCode::ParseError,
                "{text:?}"
            );
        }
        assert!(Version::parse("99999999999").is_err(), "out of range");
    }

    #[test]
    fn orders_by_components() {
        assert!(Version::parse("v2412").expect("valid") < Version::parse("v2606").expect("valid"));
        assert!(Version::parse("9.4.2").expect("valid") < Version::parse("9.10").expect("valid"));
    }
}
