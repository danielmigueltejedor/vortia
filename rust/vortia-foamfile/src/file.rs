//! One case file: its `FoamFile` header and the body after it.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::dict::Dictionary;
use crate::error::{FoamError, Result};
use crate::token::{TokenKind, Tokenizer};

/// The largest file read into memory, compressed or not.
pub const MAX_FILE_BYTES: u64 = 4 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ascii,
    Binary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub format: Format,
    pub class: String,
    pub object: String,
    pub location: Option<String>,
    pub note: Option<String>,
    /// `arch` as written, e.g. `LSB;label=32;scalar=64`.
    pub arch: Option<String>,
}

impl Header {
    fn from_dict(d: &Dictionary) -> Result<Self> {
        let format = match d.word("format") {
            None | Some("ascii") => Format::Ascii,
            Some("binary") => Format::Binary,
            Some(other) => return Err(FoamError::Invalid(format!("unknown format '{other}'"))),
        };
        Ok(Self {
            format,
            class: d.word("class").unwrap_or_default().to_owned(),
            object: d.word("object").unwrap_or_default().to_owned(),
            location: d.word("location").map(str::to_owned),
            note: d.word("note").map(str::to_owned),
            arch: d.word("arch").map(str::to_owned),
        })
    }
}

/// A file read into memory, ready to tokenize.
pub struct FoamFile {
    pub path: PathBuf,
    bytes: Vec<u8>,
}

impl FoamFile {
    /// Reads `path`, or `path.gz` when only the compressed file exists.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.is_file() {
            let gzipped = path.extension().is_some_and(|e| e == "gz");
            return Ok(Self {
                path: path.to_owned(),
                bytes: read_capped(path, gzipped)?,
            });
        }
        let mut gz = path.as_os_str().to_owned();
        gz.push(".gz");
        let gz = PathBuf::from(gz);
        if gz.is_file() {
            return Ok(Self {
                bytes: read_capped(&gz, true)?,
                path: gz,
            });
        }
        Err(FoamError::Io {
            path: path.to_owned(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        })
    }

    pub fn from_bytes(path: impl Into<PathBuf>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
        }
    }

    /// The header, if there is one, and a tokenizer positioned after it.
    pub fn tokens(&self) -> Result<(Option<Header>, Tokenizer<'_>)> {
        let mut tok = Tokenizer::new(&self.bytes);
        let has_header = matches!(
            tok.peek()?,
            Some(t) if t.kind == TokenKind::Word("FoamFile".into())
        );
        if !has_header {
            return Ok((None, tok));
        }
        tok.next()?;
        tok.expect_punct(b'{')?;
        let d = Dictionary::parse(&mut tok, true, 1)?;
        Ok((Some(Header::from_dict(&d)?), tok))
    }

    /// The header, refusing binary files, which are not read yet.
    pub fn ascii_tokens(&self) -> Result<(Option<Header>, Tokenizer<'_>)> {
        let (header, tok) = self.tokens()?;
        if header.as_ref().is_some_and(|h| h.format == Format::Binary) {
            return Err(FoamError::Unsupported(
                "binary case files; write the case with 'writeFormat ascii'".into(),
            ));
        }
        Ok((header, tok))
    }

    /// Reads the whole file as a dictionary (controlDict, fvSchemes, fields).
    pub fn dictionary(&self) -> Result<(Option<Header>, Dictionary)> {
        let (header, mut tok) = self.ascii_tokens().map_err(|e| e.in_file(&self.path))?;
        let d = Dictionary::parse(&mut tok, false, 0).map_err(|e| e.in_file(&self.path))?;
        Ok((header, d))
    }
}

fn read_capped(path: &Path, gz: bool) -> Result<Vec<u8>> {
    let io = |source| FoamError::Io {
        path: path.to_owned(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut bytes = Vec::new();
    let read = if gz {
        flate2::read::GzDecoder::new(file)
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
    } else {
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)
    }
    .map_err(io)?;
    if read as u64 > MAX_FILE_BYTES {
        return Err(FoamError::Invalid(format!(
            "{} is larger than {} GiB",
            path.display(),
            MAX_FILE_BYTES >> 30
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn a_gz_path_is_decompressed_whether_named_or_found() {
        let dir = tempfile::tempdir().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(b"endTime 2;").unwrap();
        std::fs::write(dir.path().join("controlDict.gz"), gz.finish().unwrap()).unwrap();
        for name in ["controlDict", "controlDict.gz"] {
            let (_, d) = FoamFile::open(dir.path().join(name))
                .unwrap()
                .dictionary()
                .unwrap();
            assert_eq!(d.label("endTime"), Some(2), "{name}");
        }
    }
}
