use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong while reading a case file.
///
/// Parse errors carry the 1-based line they were found on, so a message can
/// point a person at the place to look.
#[derive(Debug, Error)]
pub enum FoamError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("line {line}: {message}")]
    Parse { line: u32, message: String },

    #[error("not supported yet: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Invalid(String),

    #[error("{}: {source}", path.display())]
    InFile {
        path: PathBuf,
        #[source]
        source: Box<FoamError>,
    },
}

impl FoamError {
    pub(crate) fn parse(line: u32, message: impl Into<String>) -> Self {
        Self::Parse {
            line,
            message: message.into(),
        }
    }

    /// Wraps the error with the file it came from, unless it already names one.
    pub fn in_file(self, path: impl Into<PathBuf>) -> Self {
        match self {
            Self::Io { .. } | Self::InFile { .. } => self,
            other => Self::InFile {
                path: path.into(),
                source: Box::new(other),
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, FoamError>;
