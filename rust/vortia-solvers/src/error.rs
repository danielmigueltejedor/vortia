use thiserror::Error;

#[derive(Debug, Error)]
pub enum SolverError {
    #[error(transparent)]
    Fv(#[from] vortia_fv::FvError),

    #[error(transparent)]
    Field(#[from] vortia_fields::FieldError),

    #[error(transparent)]
    Mesh(#[from] vortia_mesh::MeshError),

    #[error(transparent)]
    Read(#[from] vortia_foamfile::FoamError),

    #[error("{0}")]
    Setup(String),

    #[error("the run was stopped")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, SolverError>;
