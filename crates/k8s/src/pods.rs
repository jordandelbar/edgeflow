//! Cached pod listing backed by a `kube::runtime::reflector` watcher.
//!
//! [`PodCache::start`] spawns a background task that watches every pod
//! carrying the `edgeflow-target` label. Reads ([`PodCache::for_target`],
//! [`PodCache::all_grouped`]) are served from the in-memory store - no k8s
//! round-trip per call. The watch stream auto-reconnects with backoff.
//!
//! Until the initial LIST response has arrived, reads return `None` so the
//! orchestrator's "k8s unreachable" semantics are preserved. Once the cache
//! has populated at least once, reads always return `Some(...)`; transient
//! watcher errors leave the last-known state in place rather than degrading
//! to `None`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use edgeflow_core::{TargetHealth, TargetPod};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::Api;
use kube::runtime::reflector::{self, Store};
use kube::runtime::watcher::{self, Event};
use kube::runtime::WatchStreamExt;
use kube::Client;

use crate::client::{client, namespace};
use crate::naming::TARGET_LABEL;

/// Derive pod health from k8s pod status.
/// `Running + Ready=True` -> Healthy; `Running + Ready=False` -> Stale; otherwise -> Unhealthy.
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
        _ => TargetHealth::Stale,
    }
}

/// Convert a k8s `Pod` into our `TargetPod`. Returns `None` if the pod has no name.
///
/// `pod_ip` may be absent while a pod is starting; the address is left empty
/// in that case so the pod isn't silently pruned from the store.
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

fn target_label(pod: &Pod) -> Option<&str> {
    pod.metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(TARGET_LABEL))
        .map(String::as_str)
}

/// In-memory cache of inference pods, populated by a kube reflector.
pub struct PodCache {
    store: Store<Pod>,
    ready: Arc<AtomicBool>,
}

impl PodCache {
    /// Start the watcher and return a handle. Returns `None` if no kube
    /// client can be constructed (running outside k8s, missing kubeconfig).
    /// Once started, the reflector handles reconnects internally; transient
    /// errors do not require restart.
    pub async fn start() -> Option<Arc<Self>> {
        let client = client("pod_cache").await?;
        Some(Arc::new(Self::start_with(client, namespace())))
    }

    fn start_with(client: Client, ns: String) -> Self {
        let api: Api<Pod> = Api::namespaced(client, &ns);
        let cfg = watcher::Config::default().labels(TARGET_LABEL);
        let (store, writer) = reflector::store::<Pod>();
        let ready = Arc::new(AtomicBool::new(false));
        let ready_writer = ready.clone();

        let stream = watcher::watcher(api, cfg).default_backoff().reflect(writer);
        tokio::spawn(async move {
            futures::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                match event {
                    Ok(Event::InitDone) => {
                        if !ready_writer.swap(true, Ordering::Release) {
                            tracing::info!("pod cache initial sync complete");
                        }
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "pod watcher error - retrying"),
                }
            }
            tracing::warn!("pod watcher stream ended");
        });

        Self { store, ready }
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Pods for a single target. `None` until the initial sync completes.
    pub fn for_target(&self, target: &str) -> Option<Vec<TargetPod>> {
        if !self.is_ready() {
            return None;
        }
        Some(
            self.store
                .state()
                .iter()
                .filter(|pod| target_label(pod) == Some(target))
                .filter_map(|pod| pod_to_target_pod(pod))
                .collect(),
        )
    }

    /// Pods grouped by target. `None` until the initial sync completes.
    pub fn all_grouped(&self) -> Option<HashMap<String, Vec<TargetPod>>> {
        if !self.is_ready() {
            return None;
        }
        let mut map: HashMap<String, Vec<TargetPod>> = HashMap::new();
        for pod in self.store.state().iter() {
            let Some(name) = target_label(pod).map(str::to_owned) else {
                continue;
            };
            if let Some(target_pod) = pod_to_target_pod(pod) {
                map.entry(name).or_default().push(target_pod);
            }
        }
        Some(map)
    }
}
