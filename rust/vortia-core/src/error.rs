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

    /// The message followed by the context in brackets, when there is one.
    pub fn describe(&self) -> String {
        if self.context.is_empty() {
            self.message.clone()
        } else {
            format!("{} [{}]", self.message, self.context)
        }
    }
}

pub type Result<T> = std::result::Result<T, VortiaError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_appends_the_context_only_when_present() {
        assert_eq!(
            VortiaError::new(ErrorCode::IoError, "cannot open", "").describe(),
            "cannot open"
        );
        let with_context = VortiaError::new(ErrorCode::IoError, "cannot open", "/tmp/x");
        assert_eq!(with_context.describe(), "cannot open [/tmp/x]");
    }
}
