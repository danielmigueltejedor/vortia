use thiserror::Error;

#[derive(Debug, Error)]
pub enum FvError {
    #[error(transparent)]
    Field(#[from] vortia_fields::FieldError),

    #[error(transparent)]
    Mesh(#[from] vortia_mesh::MeshError),

    #[error(transparent)]
    Read(#[from] vortia_foamfile::FoamError),

    #[error("matrix shape: {0}")]
    Shape(String),

    #[error("singular matrix: {0}")]
    Singular(String),

    #[error("the solution diverged: {0}")]
    Diverged(String),

    #[error("boundary condition: {0}")]
    Condition(String),

    #[error("{0}")]
    Setup(String),
}

pub type Result<T> = std::result::Result<T, FvError>;
