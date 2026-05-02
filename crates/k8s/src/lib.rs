#![allow(rustdoc::private_intra_doc_links)]
//! k8s integration for edgeflow.
//!
//! This crate is the only place in edgeflow that imports `kube` /
//! `k8s_openapi`. Everything is grouped by domain:
//!
//! - [`deployments`] - create / read / update / delete inference Deployments
//! - [`pods`] - cached pod listing via a `kube::runtime::reflector` watcher
//! - [`nodes`] - list cluster nodes
//! - [`settings`] - apply env-var overrides + defaults to user-supplied settings
//!
//! All public functions degrade gracefully when the cluster is unreachable
//! (logging a warning and returning `None` / an empty value / `false`).

mod client;
mod deployments;
mod naming;
mod nodes;
mod pods;
mod settings;

pub use deployments::{
    create_inference_pod, delete_inference_pod, get_inference_pod_infra,
    patch_inference_pod_resources,
};
pub use nodes::list_nodes;
pub use pods::PodCache;
pub use settings::{resolve_infra, resolve_resources};
