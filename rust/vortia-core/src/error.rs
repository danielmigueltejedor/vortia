use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    None,
    NotFound,
    InvalidArgument,
    InvalidData,
    IoError,
    PermissionDenied,
    ParseError,
    ProcessFailed,
    Timeout,
    Cancelled,
    BackendUnavailable,
    UnsupportedVersion,
    UnsupportedFeature,
    Internal,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct VortiaError {
    pub code: ErrorCode,
    pub message: String,
    pub context: String,
}

impl VortiaError {
    pub fn new(code: ErrorCode, message: impl Into<String>, context: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: context.into(),
        }
    }

    pub fn describe(&self) -> String {
        if self.context.is_empty() {
            self.message.clone()
        } else {
            format!("{} [{}]", self.message, self.context)
        }
    }
}

pub type Result<T> = std::result::Result<T, VortiaError>;