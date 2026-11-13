//! The tokenizer for the ASCII case format.
//!
//! The rules follow OpenFOAM's stream reader, which this format comes from:
//! `//` and `/* */` comments, the punctuation `{ } ( ) [ ] ;`, quoted strings,
//! numbers, and words. A word may carry balanced parentheses, which is how
//! `div(phi,U)` in `fvSchemes` stays one token while `3(1 2 3)` is a count, an
//! opening parenthesis and three labels.

use crate::error::{FoamError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Punct(u8),
    Word(String),
    Str(String),
    /// A `#{ ... #}` block of code, verbatim.
    Code(String),
    Label(i64),
    Scalar(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// 1-based line the token starts on.
    pub line: u32,
}

impl Token {
    pub fn is_punct(&self, c: u8) -> bool {
        self.kind == TokenKind::Punct(c)
    }

    /// The value of a numeric token, labels included.
    pub fn as_scalar(&self) -> Option<f64> {
        match self.kind {
            TokenKind::Scalar(v) => Some(v),
            // Exact for every label below 2^53, which covers any real mesh.
            TokenKind::Label(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match &self.kind {
            TokenKind::Punct(c) => format!("'{}'", *c as char),
            TokenKind::Word(w) => format!("word '{w}'"),
            TokenKind::Str(s) => format!("string \"{s}\""),
            TokenKind::Code(_) => "a code block".to_string(),
            TokenKind::Label(v) => format!("label {v}"),
            TokenKind::Scalar(v) => format!("scalar {v}"),
        }
    }
}

const PUNCT: &[u8] = b"{}()[];";

/// The deepest nesting of parentheses accepted inside one word.
const MAX_WORD_DEPTH: u32 = 32;

pub struct Tokenizer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    peeked: Option<Token>,
    /// Values uniform lists (`N{v}`) have expanded to so far, across the
    /// whole input: each list is bounded on its own, and so is their sum.
    expanded: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            peeked: None,
            expanded: 0,
        }
    }

    /// Counts `n` more expanded values against the input's budget; `false`
    /// once the total would pass `limit`.
    pub fn spend_expansion(&mut self, n: usize, limit: usize) -> bool {
        match self.expanded.checked_add(n).filter(|&t| t <= limit) {
            Some(t) => {
                self.expanded = t;
                true
            }
            None => false,
        }
    }

    pub fn line(&self) -> u32 {
        self.peeked.as_ref().map_or(self.line, |t| t.line)
    }

    /// Bytes not yet read; a bound on how many items can still follow.
    pub fn remaining(&self) -> usize {
        self.src.len().saturating_sub(self.pos)
    }

    pub fn peek(&mut self) -> Result<Option<&Token>> {
        if self.peeked.is_none() {
            self.peeked = self.read()?;
        }
        Ok(self.peeked.as_ref())
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Token>> {
        match self.peeked.take() {
            Some(t) => Ok(Some(t)),
            None => self.read(),
        }
    }

    /// The next token, or an error naming what was expected.
    pub fn expect_any(&mut self, what: &str) -> Result<Token> {
        let line = self.line();
        self.next()?
            .ok_or_else(|| FoamError::parse(line, format!("expected {what}, found end of file")))
    }

    pub fn expect_punct(&mut self, c: u8) -> Result<()> {
        let t = self.expect_any(&format!("'{}'", c as char))?;
        if t.is_punct(c) {
            Ok(())
        } else {
            Err(FoamError::parse(
                t.line,
                format!("expected '{}', found {}", c as char, t.describe()),
            ))
        }
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.src.get(at).copied()
    }

    fn skip_space_and_comments(&mut self) -> Result<()> {
        loop {
            let Some(c) = self.byte(self.pos) else {
                return Ok(());
            };
            if c == b'\n' {
                self.line = self.line.saturating_add(1);
                self.pos += 1;
            } else if c.is_ascii_whitespace() {
                self.pos += 1;
            } else if c == b'/' && self.byte(self.pos + 1) == Some(b'/') {
                while let Some(c) = self.byte(self.pos) {
                    if c == b'\n' {
                        break;
                    }
                    self.pos += 1;
                }
            } else if c == b'/' && self.byte(self.pos + 1) == Some(b'*') {
                let start = self.line;
                self.pos += 2;
                loop {
                    match self.byte(self.pos) {
                        None => return Err(FoamError::parse(start, "comment is never closed")),
                        Some(b'*') if self.byte(self.pos + 1) == Some(b'/') => {
                            self.pos += 2;
                            break;
                        }
                        Some(b'\n') => {
                            self.line = self.line.saturating_add(1);
                            self.pos += 1;
                        }
                        Some(_) => self.pos += 1,
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    fn read(&mut self) -> Result<Option<Token>> {
        self.skip_space_and_comments()?;
        let line = self.line;
        let Some(c) = self.byte(self.pos) else {
            return Ok(None);
        };
        let kind = if PUNCT.contains(&c) {
            self.pos += 1;
            TokenKind::Punct(c)
        } else if c == b'"' {
            TokenKind::Str(self.read_string(line)?)
        } else if c == b'#' && self.byte(self.pos + 1) == Some(b'{') {
            TokenKind::Code(self.read_code(line)?)
        } else if starts_number(c, self.byte(self.pos + 1)) {
            self.read_number()
        } else {
            TokenKind::Word(self.read_word())
        };
        Ok(Some(Token { kind, line }))
    }

    /// The text between `#{` and `#}`, as it is.
    fn read_code(&mut self, line: u32) -> Result<String> {
        self.pos += 2;
        let start = self.pos;
        loop {
            match self.byte(self.pos) {
                None => return Err(FoamError::parse(line, "code block is never closed")),
                Some(b'#') if self.byte(self.pos + 1) == Some(b'}') => {
                    let text = self.src.get(start..self.pos).unwrap_or_default();
                    self.pos += 2;
                    return Ok(String::from_utf8_lossy(text).into_owned());
                }
                Some(c) => {
                    if c == b'\n' {
                        self.line = self.line.saturating_add(1);
                    }
                    self.pos += 1;
                }
            }
        }
    }

    fn read_string(&mut self, line: u32) -> Result<String> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            match self.byte(self.pos) {
                None => return Err(FoamError::parse(line, "string is never closed")),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    match self.byte(self.pos + 1) {
                        Some(b'"') => out.push(b'"'),
                        Some(b'\\') => out.push(b'\\'),
                        // A backslash before a newline continues the string.
                        Some(b'\n') => self.line = self.line.saturating_add(1),
                        Some(other) => out.extend_from_slice(&[b'\\', other]),
                        None => return Err(FoamError::parse(line, "string is never closed")),
                    }
                    self.pos += 2;
                }
                Some(c) => {
                    if c == b'\n' {
                        self.line = self.line.saturating_add(1);
                    }
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Whether a `//` or `/*` comment begins at `pos`: it ends the token before it.
    fn comment_at(&self, pos: usize) -> bool {
        self.byte(pos) == Some(b'/') && matches!(self.byte(pos + 1), Some(b'/' | b'*'))
    }

    fn read_number(&mut self) -> TokenKind {
        let start = self.pos;
        while let Some(c) = self.byte(self.pos) {
            if c.is_ascii_whitespace()
                || PUNCT.contains(&c)
                || c == b'"'
                || self.comment_at(self.pos)
            {
                break;
            }
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(self.src.get(start..self.pos).unwrap_or_default());
        if let Ok(v) = text.parse::<i64>() {
            TokenKind::Label(v)
        } else if let Ok(v) = text.parse::<f64>() {
            TokenKind::Scalar(v)
        } else {
            TokenKind::Word(text.into_owned())
        }
    }

    fn read_word(&mut self) -> String {
        let start = self.pos;
        let mut depth = 0u32;
        while let Some(c) = self.byte(self.pos) {
            if c.is_ascii_whitespace()
                || c == b'"'
                || matches!(c, b'{' | b'}' | b';' | b'[' | b']')
                || self.comment_at(self.pos)
            {
                break;
            }
            if c == b'(' {
                if depth >= MAX_WORD_DEPTH {
                    break;
                }
                depth += 1;
            } else if c == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            self.pos += 1;
        }
        String::from_utf8_lossy(self.src.get(start..self.pos).unwrap_or_default()).into_owned()
    }
}

fn starts_number(c: u8, next: Option<u8>) -> bool {
    let digit_or_dot = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_digit() || b == b'.');
    c.is_ascii_digit()
        || (matches!(c, b'-' | b'+') && digit_or_dot(next))
        || (c == b'.' && next.is_some_and(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let mut t = Tokenizer::new(src.as_bytes());
        let mut out = Vec::new();
        while let Some(tok) = t.next().unwrap() {
            out.push(tok.kind);
        }
        out
    }

    #[test]
    fn words_keep_balanced_parentheses() {
        assert_eq!(
            kinds("div(phi,U) Gauss linear;"),
            vec![
                TokenKind::Word("div(phi,U)".into()),
                TokenKind::Word("Gauss".into()),
                TokenKind::Word("linear".into()),
                TokenKind::Punct(b';'),
            ]
        );
    }

    #[test]
    fn counted_lists_split_into_count_and_items() {
        assert_eq!(
            kinds("3(1 -2 2.5e-3)"),
            vec![
                TokenKind::Label(3),
                TokenKind::Punct(b'('),
                TokenKind::Label(1),
                TokenKind::Label(-2),
                TokenKind::Scalar(2.5e-3),
                TokenKind::Punct(b')'),
            ]
        );
    }

    #[test]
    fn comments_are_skipped_and_lines_counted() {
        let mut t = Tokenizer::new(b"// one\n/* two\nthree */ word");
        let tok = t.next().unwrap().unwrap();
        assert_eq!(tok.kind, TokenKind::Word("word".into()));
        assert_eq!(tok.line, 3);
    }

    #[test]
    fn a_comment_right_after_a_token_ends_it() {
        assert_eq!(
            kinds("1/*c*/ 0// note\n2.5/*x*/word/*y*/;"),
            vec![
                TokenKind::Label(1),
                TokenKind::Label(0),
                TokenKind::Scalar(2.5),
                TokenKind::Word("word".into()),
                TokenKind::Punct(b';'),
            ]
        );
    }

    #[test]
    fn unterminated_comment_and_string_are_errors() {
        assert!(Tokenizer::new(b"/* open").next().is_err());
        assert!(Tokenizer::new(b"\"open").next().is_err());
    }

    #[test]
    fn hostile_bytes_do_not_panic() {
        let inputs: [&[u8]; 6] = [
            b"\xff\xfe(((",
            b"-",
            b"+.",
            b"\"\\",
            b"((((((((((((((((((((((((((((((((((((a",
            b"/",
        ];
        for input in inputs {
            let mut t = Tokenizer::new(input);
            while let Ok(Some(_)) = t.next() {}
        }
    }
}
