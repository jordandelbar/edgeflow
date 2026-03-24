pub mod artifact;
pub mod experiment;
pub mod metric;
pub mod run;

pub use experiment::*;
pub use run::*;
pub use metric::*;
pub use artifact::*;

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
