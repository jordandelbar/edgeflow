//! Production orchestrator: thin wrapper over `edgeflow-k8s`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use edgeflow_core::{InfraSettings, ResourceSettings, TargetPod};
use edgeflow_k8s::PodCache;

use crate::Orchestrator;

#[derive(Default)]
pub struct K8sOrchestrator {
    pod_cache: Option<Arc<PodCache>>,
}

impl K8sOrchestrator {
    pub async fn new() -> Self {
        Self {
            pod_cache: PodCache::start().await,
        }
    }
}

#[async_trait]
impl Orchestrator for K8sOrchestrator {
    async fn create_inference_pod(
        &self,
        target: &str,
        node: Option<&str>,
        resources: &ResourceSettings,
        infra: &InfraSettings,
    ) -> anyhow::Result<()> {
        edgeflow_k8s::create_inference_pod(target, node, resources, infra).await;
        Ok(())
    }

    async fn delete_inference_pod(&self, target: &str) {
        edgeflow_k8s::delete_inference_pod(target).await
    }

    async fn patch_inference_pod_resources(
        &self,
        target: &str,
        resources: Option<&ResourceSettings>,
        infra: Option<&InfraSettings>,
    ) -> bool {
        edgeflow_k8s::patch_inference_pod_resources(target, resources, infra).await
    }

    async fn get_inference_pod_infra(&self, target: &str) -> Option<InfraSettings> {
        edgeflow_k8s::get_inference_pod_infra(target).await
    }

    async fn list_running_pods(&self, target: &str) -> Option<Vec<TargetPod>> {
        self.pod_cache.as_ref()?.for_target(target)
    }

    async fn list_all_running_pods(&self) -> Option<HashMap<String, Vec<TargetPod>>> {
        self.pod_cache.as_ref()?.all_grouped()
    }

    async fn list_nodes(&self) -> Vec<String> {
        edgeflow_k8s::list_nodes().await
    }
}
