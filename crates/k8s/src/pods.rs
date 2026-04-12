//! List inference pods, populated with k8s-derived address/node/health.

use std::collections::HashMap;

use edgeflow_core::{TargetHealth, TargetPod};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams};

use crate::client::{client, namespace};

/// Derive pod health from k8s pod status.
/// `Running + Ready=True` → Healthy; `Running + Ready=False` → Stale; otherwise → Unhealthy.
fn health_from_pod(pod: &Pod) -> TargetHealth {
    let phase = pod
        .status
        .as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Unknown");
    let ready = pod
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|cs| cs.iter().find(|c| c.type_ == "Ready"))
        .map(|c| c.status == "True")
        .unwrap_or(false);
    match phase {
        "Running" if ready => TargetHealth::Healthy,
        "Running" => TargetHealth::Stale,
        "Failed" | "Unknown" => TargetHealth::Unhealthy,
        _ => TargetHealth::Stale, // Pending or other transient states
    }
}

/// Convert a k8s `Pod` into our `TargetPod`. Returns `None` if the pod has no name.
///
/// `pod_ip` may be absent while a pod is starting; the address is left empty in
/// that case so the pod isn't silently pruned from the store.
fn pod_to_target_pod(pod: &Pod) -> Option<TargetPod> {
    let pod_id = pod.metadata.name.clone()?;
    let address = pod
        .status
        .as_ref()
        .and_then(|s| s.pod_ip.as_ref())
        .map(|ip| format!("http://{}:8080", ip))
        .unwrap_or_default();
    let node = pod.spec.as_ref().and_then(|s| s.node_name.clone());
    let registered_at = pod
        .metadata
        .creation_timestamp
        .as_ref()
        .map(|t| t.0.timestamp_millis())
        .unwrap_or(0);
    let health = health_from_pod(pod);
    Some(TargetPod {
        pod_id,
        address,
        node,
        registered_at,
        health,
    })
}

/// List running pods for `target`, populated with k8s-derived address/node/health.
/// Returns `None` when k8s is unreachable.
pub async fn list_running_pods(target: &str) -> Option<Vec<TargetPod>> {
    let client = client("list_running_pods").await?;
    let api: Api<Pod> = Api::namespaced(client, &namespace());
    let lp = ListParams::default().labels(&format!("edgeflow-target={target}"));
    let pods = api
        .list(&lp)
        .await
        .map_err(|e| tracing::warn!(target = %target, error = %e, "failed to list pods"))
        .ok()?;
    Some(pods.items.iter().filter_map(pod_to_target_pod).collect())
}

/// List running pods for ALL edgeflow targets in one k8s call.
/// Returns a map of `target_name → Vec<TargetPod>`, or `None` when k8s is unreachable.
pub async fn list_all_running_pods() -> Option<HashMap<String, Vec<TargetPod>>> {
    let client = client("list_all_running_pods").await?;
    let api: Api<Pod> = Api::namespaced(client, &namespace());
    let lp = ListParams::default().labels("edgeflow-target");
    let pods = api
        .list(&lp)
        .await
        .map_err(|e| tracing::warn!(error = %e, "failed to list all pods"))
        .ok()?;
    let mut map: HashMap<String, Vec<TargetPod>> = HashMap::new();
    for pod in &pods.items {
        let Some(target_name) = pod
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get("edgeflow-target"))
            .cloned()
        else {
            continue;
        };
        if let Some(target_pod) = pod_to_target_pod(pod) {
            map.entry(target_name).or_default().push(target_pod);
        }
    }
    Some(map)
}
