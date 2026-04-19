//! Demo orchestrator for the docker compose path.
//!
//! Compose mode runs a single pre-provisioned inference container. The
//! orchestrator is target-agnostic: it returns a synthetic [`TargetPod`] for
//! whatever target the caller asks about, and all management operations are
//! no-ops. The single pod picks up upgrade commands for any target via a
//! wildcard MQTT subscription (`edgeflow/targets/+/commands`).

use std::collections::HashMap;

use async_trait::async_trait;
use edgeflow_core::{InfraSettings, ResourceSettings, TargetHealth, TargetPod};

use crate::Orchestrator;

/// Configuration for the compose orchestrator. Built from environment in
/// `apps/server/src/main.rs` when `EDGEFLOW_ORCHESTRATOR=compose`.
pub struct ComposeOrchestrator {
    address: String,
}

impl ComposeOrchestrator {
    /// `address` is the URL the server uses to reach the pre-provisioned
    /// inference container (e.g. `http://inference:8080`).
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
        }
    }

    /// Build a synthetic `TargetPod` for the given target name.
    fn synthetic_pod(&self, target: &str) -> TargetPod {
        TargetPod {
            pod_id: format!("{target}-0"),
            address: self.address.clone(),
            node: Some("compose".into()),
            registered_at: 0,
            health: TargetHealth::Healthy,
        }
    }
}

#[async_trait]
impl Orchestrator for ComposeOrchestrator {
    async fn create_inference_pod(
        &self,
        _target: &str,
        _node: Option<&str>,
        _resources: &ResourceSettings,
        _infra: &InfraSettings,
    ) -> anyhow::Result<()> {
        // The container is already running and accepts any target via
        // wildcard MQTT subscribe. Nothing to provision.
        Ok(())
    }

    async fn delete_inference_pod(&self, target: &str) {
        tracing::warn!(
            target,
            "compose mode does not support tearing down inference pods at runtime; \
             stop the container with `docker compose down` instead"
        );
    }

    async fn patch_inference_pod_resources(
        &self,
        _target: &str,
        _resources: Option<&ResourceSettings>,
        _infra: Option<&InfraSettings>,
    ) -> bool {
        tracing::warn!(
            "compose mode does not support patching pod resources at runtime; \
             edit docker-compose.yaml and restart"
        );
        false
    }

    async fn get_inference_pod_infra(&self, _target: &str) -> Option<InfraSettings> {
        None
    }

    async fn list_running_pods(&self, target: &str) -> Option<Vec<TargetPod>> {
        // Report a synthetic pod for any target the caller asks about.
        // The single compose pod serves all targets.
        Some(vec![self.synthetic_pod(target)])
    }

    async fn list_all_running_pods(&self) -> Option<HashMap<String, Vec<TargetPod>>> {
        // Compose doesn't know which targets exist - that's the store's job.
        // Returning None tells the handler to fall back to per-target iteration.
        None
    }

    async fn list_nodes(&self) -> Vec<String> {
        vec!["compose".into()]
    }
}
