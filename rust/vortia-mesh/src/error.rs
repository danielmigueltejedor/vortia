use thiserror::Error;

#[derive(Debug, Error)]
pub enum MeshError {
    #[error(transparent)]
    Read(#[from] vortia_foamfile::FoamError),

    #[error("invalid mesh: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, MeshError>;
