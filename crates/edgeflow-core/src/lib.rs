pub mod artifact;
pub mod deployment;
pub mod experiment;
pub mod metric;
pub mod run;

pub use artifact::*;
pub use deployment::*;
pub use experiment::*;
pub use metric::*;
pub use run::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}
