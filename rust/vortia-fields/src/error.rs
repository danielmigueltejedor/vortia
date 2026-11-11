use thiserror::Error;

#[derive(Debug, Error)]
pub enum FieldError {
    #[error(transparent)]
    Read(#[from] vortia_foamfile::FoamError),

    #[error("{0}")]
    Invalid(String),

    /// The field does not fit the mesh it is meant for.
    #[error("{field}: {message}")]
    Mismatch { field: String, message: String },
}

pub type Result<T> = std::result::Result<T, FieldError>;
