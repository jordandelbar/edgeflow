//! Orchestrator abstraction over the inference-pod runtime.
//!
//! edgeflow runs inference pods on different substrates: a full k8s cluster
//! for production / multi-target deployments, and a single-process docker
//! compose stack for the local demo. Both expose the same management
//! operations behind the [`Orchestrator`] trait so the API handlers in the
//! server are runtime-agnostic.
//!
//! The compose backend is intentionally barebones: it only knows about a
//! single pre-provisioned inference container. Operations that don't make
//! sense in that model (creating new targets, patching resources, etc.) log
//! a warning and no-op.

use std::collections::HashMap;

use async_trait::async_trait;
use edgeflow_core::{InfraSettings, ResourceSettings, TargetPod};

mod compose;
mod k8s;

pub use compose::ComposeOrchestrator;
pub use k8s::K8sOrchestrator;

// Re-export the pure default-resolution helpers so callers don't need to
// depend on edgeflow-k8s directly.
pub use edgeflow_k8s::{resolve_infra, resolve_resources};

#[async_trait]
pub trait Orchestrator: Send + Sync {
    /// Provision an inference pod for `target`. May be a no-op when the
    /// orchestrator uses pre-provisioned pods (compose mode).
    ///
    /// Returns an error when the orchestrator cannot satisfy the request,
    /// e.g. when compose mode is asked to provision a target that differs
    /// from the one the inference container is pinned to. Error messages
    /// containing the word `unsupported` are mapped to HTTP 400 by the API
    /// layer so users see a clear validation failure rather than a 500.
    async fn create_inference_pod(
        &self,
        target: &str,
        node: Option<&str>,
        resources: &ResourceSettings,
        infra: &InfraSettings,
    ) -> anyhow::Result<()>;

    /// Tear down the inference pod for `target`.
    async fn delete_inference_pod(&self, target: &str);

    /// Update resource requests/limits and replica count. Returns `true` when
    /// the patch was accepted, `false` when the orchestrator can't apply it.
    async fn patch_inference_pod_resources(
        &self,
        target: &str,
        resources: Option<&ResourceSettings>,
        infra: Option<&InfraSettings>,
    ) -> bool;

    /// Read current infra settings from the running deployment, or `None` when
    /// the orchestrator can't introspect them.
    async fn get_inference_pod_infra(&self, target: &str) -> Option<InfraSettings>;

    /// List running pods for `target`. `None` indicates the runtime is
    /// unreachable; `Some(vec![])` means "reachable, no pods".
    async fn list_running_pods(&self, target: &str) -> Option<Vec<TargetPod>>;

    /// List running pods for every target the orchestrator manages.
    ///
    /// Returns `None` when the orchestrator cannot enumerate targets on its
    /// own (e.g. compose mode, where targets are a store-level concept). When
    /// `None`, callers should fall back to iterating `list_running_pods` over
    /// the store's known targets.
    async fn list_all_running_pods(&self) -> Option<HashMap<String, Vec<TargetPod>>>;

    /// List node names. For non-clustered orchestrators this is a single
    /// synthetic name (e.g. `"compose"`).
    async fn list_nodes(&self) -> Vec<String>;
}
