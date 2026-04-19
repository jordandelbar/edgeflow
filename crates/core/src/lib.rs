//! Core domain types for the [edgeflow](https://github.com/jordandelbar/edgeflow)
//! inference platform.
//!
//! This crate defines the serializable types that flow between the edgeflow
//! server, its clients, and pluggable storage backends: experiments, runs,
//! metrics, registered models, model versions, deployments, and targets.
//!
//! It performs no I/O and depends only on `serde`, `chrono`, `uuid`, and
//! `thiserror` - downstream crates consume these types to build alternative
//! servers, clients, or storage backends.
//!
//! # Example
//!
//! ```
//! use edgeflow_core::{Experiment, LifecycleStage};
//!
//! let exp = Experiment {
//!     experiment_id: "0".into(),
//!     name: "iris-classifier".into(),
//!     artifact_location: "/artifacts/0".into(),
//!     lifecycle_stage: LifecycleStage::Active,
//!     creation_time: 1_712_000_000_000,
//!     last_update_time: 1_712_000_000_000,
//!     tags: vec![],
//! };
//!
//! let json = serde_json::to_string(&exp).unwrap();
//! assert!(json.contains("iris-classifier"));
//! ```

pub mod artifact;
pub mod deployment;
pub mod experiment;
pub mod metric;
pub mod model_registry;
pub mod run;

pub use artifact::*;
pub use deployment::*;
pub use experiment::*;
pub use metric::*;
pub use model_registry::*;
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
