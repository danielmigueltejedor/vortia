use thiserror::Error;

#[derive(Debug, Error)]
pub enum PostError {
    #[error(transparent)]
    Mesh(#[from] vortia_mesh::MeshError),

    #[error(transparent)]
    Field(#[from] vortia_fields::FieldError),

    #[error(transparent)]
    File(#[from] vortia_foamfile::FoamError),

    /// What was asked for does not exist or does not make sense here.
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, PostError>;
