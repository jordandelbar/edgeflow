use std::collections::BTreeMap;

use edgeflow_core::{InfraSettings, Placement, ResourceSettings, TargetHealth, TargetPod};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Affinity, Container, ContainerPort, EnvVar, EnvVarSource, HTTPGetAction, ObjectFieldSelector,
    PodAffinity, PodAffinityTerm, PodAntiAffinity, PodSpec, PodTemplateSpec, Probe,
    ResourceRequirements, WeightedPodAffinityTerm,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, DeleteParams, ListParams, PostParams};

fn label_selector(target: &str) -> LabelSelector {
    LabelSelector {
        match_labels: Some([("edgeflow-target".to_string(), target.to_string())].into()),
        ..Default::default()
    }
}

fn affinity_term(target: &str) -> PodAffinityTerm {
    PodAffinityTerm {
        label_selector: Some(label_selector(target)),
        topology_key: "kubernetes.io/hostname".to_string(),
        ..Default::default()
    }
}

/// Anti-affinity: prefer scheduling each replica on a different node.
fn spread_affinity(target: &str) -> Affinity {
    Affinity {
        pod_anti_affinity: Some(PodAntiAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(vec![
                WeightedPodAffinityTerm {
                    weight: 100,
                    pod_affinity_term: affinity_term(target),
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Affinity: prefer scheduling all replicas on the same node.
fn pack_affinity(target: &str) -> Affinity {
    Affinity {
        pod_affinity: Some(PodAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(vec![
                WeightedPodAffinityTerm {
                    weight: 100,
                    pod_affinity_term: affinity_term(target),
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn placement_affinity(placement: &Placement, target: &str) -> Affinity {
    match placement {
        Placement::Spread => spread_affinity(target),
        Placement::Pack => pack_affinity(target),
    }
}

/// Sanitise a target name into a valid k8s resource name.
/// k8s names: lowercase alphanumeric + `-`, max 63 chars.
fn k8s_name(target: &str) -> String {
    let sanitized: String = target
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!(
        "edgeflow-inference-{}",
        &sanitized[..sanitized.len().min(45)]
    )
}

/// Resolve effective edgeflow resource settings by applying env-var overrides
/// and hardcoded defaults. Returns a fully-populated `ResourceSettings`.
pub fn resolve_resources(resources: &ResourceSettings) -> ResourceSettings {
    let sessions = resources
        .sessions
        .or_else(|| {
            std::env::var("EDGEFLOW_SESSIONS")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(1);
    let max_concurrent = resources
        .max_concurrent
        .or_else(|| {
            std::env::var("EDGEFLOW_MAX_CONCURRENT_INFER")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(sessions);
    ResourceSettings {
        sessions: Some(sessions),
        max_concurrent: Some(max_concurrent),
    }
}

/// Resolve effective k8s infrastructure settings by applying env-var overrides
/// and hardcoded defaults for cpu/memory. Replica/spread/node_selector are
/// left as-is (None means "not set by the user").
pub fn resolve_infra(infra: &InfraSettings) -> InfraSettings {
    let cpu_request = infra
        .cpu_request
        .clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_CPU_REQUEST").ok())
        .unwrap_or_else(|| "100m".into());
    let memory_request = infra
        .memory_request
        .clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_MEMORY_REQUEST").ok())
        .unwrap_or_else(|| "256Mi".into());
    let memory_limit = infra
        .memory_limit
        .clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_MEMORY_LIMIT").ok())
        .unwrap_or_else(|| "512Mi".into());
    InfraSettings {
        cpu_request: Some(cpu_request),
        memory_request: Some(memory_request),
        memory_limit: Some(memory_limit),
        replicas: infra.replicas,
        placement: infra.placement.clone(),
        node_selector: infra.node_selector.clone(),
    }
}

/// Read infrastructure settings for `target` from the k8s Deployment spec.
/// Returns `None` when k8s is unreachable or the Deployment doesn't exist.
pub async fn get_inference_pod_infra(target: &str) -> Option<InfraSettings> {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());
    let client = kube::Client::try_default()
        .await
        .map_err(|e| tracing::warn!(error = %e, "k8s client unavailable (get_inference_pod_infra)"))
        .ok()?;
    let api: kube::api::Api<Deployment> = kube::api::Api::namespaced(client, &namespace);
    let name = k8s_name(target);
    let dep = api.get(&name).await
        .map_err(|e| tracing::warn!(target = %target, error = %e, "failed to get inference deployment infra"))
        .ok()?;
    let spec = dep.spec?;

    let replicas = spec.replicas;
    let pod_spec = spec.template.spec?;

    let cpu_request = pod_spec
        .containers
        .first()
        .and_then(|c| c.resources.as_ref())
        .and_then(|r| r.requests.as_ref())
        .and_then(|m| m.get("cpu"))
        .map(|q| q.0.clone());
    let memory_request = pod_spec
        .containers
        .first()
        .and_then(|c| c.resources.as_ref())
        .and_then(|r| r.requests.as_ref())
        .and_then(|m| m.get("memory"))
        .map(|q| q.0.clone());
    let memory_limit = pod_spec
        .containers
        .first()
        .and_then(|c| c.resources.as_ref())
        .and_then(|r| r.limits.as_ref())
        .and_then(|m| m.get("memory"))
        .map(|q| q.0.clone());

    let node_selector = pod_spec
        .node_selector
        .map(|m| m.into_iter().collect::<std::collections::BTreeMap<_, _>>());

    let placement = pod_spec.affinity.as_ref().and_then(|a| {
        let has_anti = a
            .pod_anti_affinity
            .as_ref()
            .and_then(|pa| {
                pa.preferred_during_scheduling_ignored_during_execution
                    .as_ref()
            })
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let has_pack = a
            .pod_affinity
            .as_ref()
            .and_then(|pa| {
                pa.preferred_during_scheduling_ignored_during_execution
                    .as_ref()
            })
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if has_anti {
            Some(Placement::Spread)
        } else if has_pack {
            Some(Placement::Pack)
        } else {
            None
        }
    });

    Some(InfraSettings {
        cpu_request,
        memory_request,
        memory_limit,
        replicas,
        placement,
        node_selector,
    })
}

/// Create a k8s Deployment for an inference pod serving `target`.
///
/// `node` pins the pod to a specific node by name (k3s/k3d node names
/// like `k3d-cluster-agent-0`). Pass `None` to let the scheduler decide.
///
/// No-ops gracefully if:
/// - the cluster is unreachable (local dev without k8s)
/// - the Deployment already exists (pod is starting but hasn't registered yet)
pub async fn create_inference_pod(
    target: &str,
    node: Option<&str>,
    resources: &ResourceSettings,
    infra: &InfraSettings,
) {
    let image = std::env::var("EDGEFLOW_INFERENCE_IMAGE")
        .unwrap_or_else(|_| "edgeflow-inference:latest".into());
    let server_url = std::env::var("EDGEFLOW_SERVER_URL")
        .unwrap_or_else(|_| "http://edgeflow-server:5000".into());
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());
    let pull_policy =
        std::env::var("EDGEFLOW_IMAGE_PULL_POLICY").unwrap_or_else(|_| "IfNotPresent".into());
    // MQTT URL for upgrade fan-out. Forward an external broker URL if configured;
    // otherwise point pods at the embedded broker running in the server pod.
    let mqtt_url = std::env::var("EDGEFLOW_MQTT_URL").unwrap_or_else(|_| {
        let port = std::env::var("EDGEFLOW_MQTT_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1883);
        format!("mqtt://edgeflow-server:{port}")
    });

    let resolved_infra = resolve_infra(infra);
    let cpu_request = resolved_infra.cpu_request.unwrap();
    let memory_request = resolved_infra.memory_request.unwrap();
    let memory_limit = resolved_infra.memory_limit.unwrap();
    let resolved_res = resolve_resources(resources);
    let sessions = resolved_res.sessions.unwrap();
    let max_concurrent = resolved_res.max_concurrent.unwrap();
    // No CPU limit — throttling degrades inference latency more than an OOM would.

    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target = %target,
                error = %e,
                "k8s client unavailable — start the inference pod manually with \
                 EDGEFLOW_SERVER={server_url} EDGEFLOW_TARGET={target}"
            );
            return;
        }
    };

    let name = k8s_name(target);
    let mut labels = BTreeMap::new();
    labels.insert("app".to_string(), name.clone());
    labels.insert("edgeflow-target".to_string(), target.to_string());

    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            namespace: Some(namespace.clone()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(resolved_infra.replicas.unwrap_or(1)),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    node_name: node.map(String::from),
                    node_selector: resolved_infra
                        .node_selector
                        .clone()
                        .map(|m| m.into_iter().collect::<BTreeMap<String, String>>()),
                    affinity: resolved_infra
                        .placement
                        .as_ref()
                        .map(|p| placement_affinity(p, target)),
                    containers: vec![Container {
                        name: "edgeflow-inference".to_string(),
                        image: Some(image.clone()),
                        image_pull_policy: Some(pull_policy),
                        ports: Some(vec![ContainerPort {
                            container_port: 8080,
                            ..Default::default()
                        }]),
                        env: Some(vec![
                            EnvVar {
                                name: "EDGEFLOW_SERVER".to_string(),
                                value: Some(server_url.clone()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_TARGET".to_string(),
                                value: Some(target.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_INFER_ADDR".to_string(),
                                value: Some("0.0.0.0:8080".to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_SESSIONS".to_string(),
                                value: Some(sessions.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_MAX_CONCURRENT_INFER".to_string(),
                                value: Some(max_concurrent.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_POD_NAME".to_string(),
                                value_from: Some(EnvVarSource {
                                    field_ref: Some(ObjectFieldSelector {
                                        field_path: "metadata.name".to_string(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_POD_IP".to_string(),
                                value_from: Some(EnvVarSource {
                                    field_ref: Some(ObjectFieldSelector {
                                        field_path: "status.podIP".to_string(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_NODE_NAME".to_string(),
                                value_from: Some(EnvVarSource {
                                    field_ref: Some(ObjectFieldSelector {
                                        field_path: "spec.nodeName".to_string(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "LOG_FORMAT".to_string(),
                                value: Some(
                                    std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".into()),
                                ),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "EDGEFLOW_MQTT_URL".to_string(),
                                value: Some(mqtt_url),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
                                value: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
                                ..Default::default()
                            },
                        ]),
                        resources: Some(ResourceRequirements {
                            requests: Some(
                                [
                                    ("cpu".into(), Quantity(cpu_request)),
                                    ("memory".into(), Quantity(memory_request)),
                                ]
                                .into(),
                            ),
                            limits: Some([("memory".into(), Quantity(memory_limit))].into()),
                            ..Default::default()
                        }),
                        readiness_probe: Some(Probe {
                            http_get: Some(HTTPGetAction {
                                path: Some("/health".to_string()),
                                port: IntOrString::Int(8080),
                                ..Default::default()
                            }),
                            initial_delay_seconds: Some(2),
                            period_seconds: Some(5),
                            failure_threshold: Some(60),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    let api: Api<Deployment> = Api::namespaced(client, &namespace);
    match api.create(&PostParams::default(), &deployment).await {
        Ok(_) => {
            tracing::info!(
                target = %target,
                name = %name,
                image = %image,
                node = ?node,
                "created inference deployment"
            );
        }
        Err(kube::Error::Api(e)) if e.code == 409 => {
            tracing::info!(
                target = %target,
                name = %name,
                "inference deployment already exists — waiting for pod to register"
            );
        }
        Err(e) => {
            tracing::error!(
                target = %target,
                error = %e,
                "failed to create inference deployment"
            );
        }
    }
}

/// Update the resource requests/limits on the k8s Deployment for `target`
/// using a read-modify-write cycle, then trigger a rolling restart.
///
/// `resources` carries edgeflow-owned fields (sessions, max_concurrent);
/// `infra` carries k8s-owned fields (cpu/memory/replicas/spread/node_selector).
/// Either can be `None` — only the provided parts are changed.
///
/// Returns `true` if the update was accepted, `false` if the cluster is
/// unreachable, the Deployment doesn't exist, or the update failed.
pub async fn patch_inference_pod_resources(
    target: &str,
    resources: Option<&ResourceSettings>,
    infra: Option<&InfraSettings>,
) -> bool {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());

    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target = %target, error = %e, "k8s client unavailable — cannot update resources");
            return false;
        }
    };

    let name = k8s_name(target);
    let api: Api<Deployment> = Api::namespaced(client, &namespace);

    // GET the current Deployment so we preserve all existing fields and have
    // the correct resourceVersion for optimistic concurrency.
    let mut deployment = match api.get(&name).await {
        Ok(d) => d,
        Err(kube::Error::Api(e)) if e.code == 404 => {
            tracing::warn!(
                target = %target,
                name = %name,
                "inference deployment not found — cannot update resources (was it created via k8s?)"
            );
            return false;
        }
        Err(e) => {
            tracing::error!(target = %target, error = %e, "failed to get inference deployment");
            return false;
        }
    };

    // Find the edgeflow-inference container and update its resources + env vars.
    let updated = (|| {
        let container = deployment
            .spec
            .as_mut()?
            .template
            .spec
            .as_mut()?
            .containers
            .iter_mut()
            .find(|c| c.name == "edgeflow-inference")?;

        if let Some(inf) = infra {
            let resolved = resolve_infra(inf);
            let cpu = resolved.cpu_request.unwrap_or_else(|| "100m".into());
            let mem_req = resolved.memory_request.unwrap_or_else(|| "256Mi".into());
            let mem_lim = resolved.memory_limit.unwrap_or_else(|| "512Mi".into());
            container.resources = Some(ResourceRequirements {
                requests: Some(
                    [
                        ("cpu".into(), Quantity(cpu)),
                        ("memory".into(), Quantity(mem_req)),
                    ]
                    .into(),
                ),
                limits: Some([("memory".into(), Quantity(mem_lim))].into()),
                ..Default::default()
            });
        }

        // Update edgeflow env vars when sessions/max_concurrent changed.
        if let Some(res) = resources {
            let resolved = resolve_resources(res);
            let sessions = resolved.sessions.unwrap_or(1);
            let max_concurrent = resolved.max_concurrent.unwrap_or(sessions);
            if let Some(env) = container.env.as_mut() {
                for var in env.iter_mut() {
                    match var.name.as_str() {
                        "EDGEFLOW_SESSIONS" => var.value = Some(sessions.to_string()),
                        "EDGEFLOW_MAX_CONCURRENT_INFER" => {
                            var.value = Some(max_concurrent.to_string())
                        }
                        _ => {}
                    }
                }
            }
        }

        Some(())
    })();

    if updated.is_none() {
        tracing::warn!(target = %target, name = %name, "edgeflow-inference container not found in deployment spec");
        return false;
    }

    // Update replica count, node_selector, and spread affinity when infra provided.
    if let Some(inf) = infra {
        if let Some(spec) = deployment.spec.as_mut() {
            if let Some(r) = inf.replicas {
                spec.replicas = Some(r);
            }
            if let Some(pod_spec) = spec.template.spec.as_mut() {
                if let Some(ns) = inf.node_selector.clone() {
                    pod_spec.node_selector = Some(ns.into_iter().collect());
                }
                // Always sync placement when infra is explicitly patched.
                // None clears any existing affinity rule; Some(...) sets it.
                pod_spec.affinity = inf
                    .placement
                    .as_ref()
                    .map(|p| placement_affinity(p, target));
            }
        }
    }

    match api
        .replace(&name, &kube::api::PostParams::default(), &deployment)
        .await
    {
        Ok(_) => {
            tracing::info!(target = %target, name = %name, "updated inference deployment resources");
            true
        }
        Err(e) => {
            tracing::error!(target = %target, error = %e, "failed to update inference deployment resources");
            false
        }
    }
}

/// Delete the k8s Deployment for `target`.
/// No-ops gracefully if the cluster is unreachable or the Deployment doesn't exist.
pub async fn delete_inference_pod(target: &str) {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());

    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target = %target,
                error = %e,
                "k8s client unavailable — delete the inference pod manually"
            );
            return;
        }
    };

    let name = k8s_name(target);
    let api: Api<k8s_openapi::api::apps::v1::Deployment> = Api::namespaced(client, &namespace);
    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => {
            tracing::info!(target = %target, name = %name, "deleted inference deployment");
        }
        Err(kube::Error::Api(e)) if e.code == 404 => {
            tracing::info!(target = %target, name = %name, "inference deployment already gone");
        }
        Err(e) => {
            tracing::error!(target = %target, error = %e, "failed to delete inference deployment");
        }
    }
}

/// Derive pod health from k8s pod status.
/// `Running + Ready=True` → Healthy; `Running + Ready=False` → Stale; otherwise → Unhealthy.
fn health_from_pod(pod: &k8s_openapi::api::core::v1::Pod) -> TargetHealth {
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

/// List running pods for `target`, populated with k8s-derived address/node/health.
/// Returns `None` when k8s is unreachable.
pub async fn list_running_pods(target: &str) -> Option<Vec<TargetPod>> {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());
    let client = kube::Client::try_default()
        .await
        .map_err(|e| tracing::warn!(error = %e, "k8s client unavailable (list_running_pods)"))
        .ok()?;
    let api: kube::api::Api<k8s_openapi::api::core::v1::Pod> =
        kube::api::Api::namespaced(client, &namespace);
    let lp = kube::api::ListParams::default().labels(&format!("edgeflow-target={target}"));
    let pods = api
        .list(&lp)
        .await
        .map_err(|e| tracing::warn!(target = %target, error = %e, "failed to list pods"))
        .ok()?;
    Some(
        pods.items
            .into_iter()
            .filter_map(|p| {
                let pod_id = p.metadata.name.clone()?;
                // pod_ip may be absent while the pod is starting; include it anyway
                // with an empty address so it isn't silently pruned from the store.
                let address = p
                    .status
                    .as_ref()
                    .and_then(|s| s.pod_ip.as_ref())
                    .map(|ip| format!("http://{}:8080", ip))
                    .unwrap_or_default();
                let node = p.spec.as_ref().and_then(|s| s.node_name.clone());
                let registered_at = p
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| t.0.timestamp_millis())
                    .unwrap_or(0);
                let health = health_from_pod(&p);
                Some(TargetPod {
                    pod_id,
                    address,
                    node,
                    registered_at,
                    health,
                })
            })
            .collect(),
    )
}

/// List running pods for ALL edgeflow targets in one k8s call.
/// Returns a map of `target_name → Vec<TargetPod>`, or `None` when k8s is unreachable.
pub async fn list_all_running_pods() -> Option<std::collections::HashMap<String, Vec<TargetPod>>> {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into());
    let client = kube::Client::try_default()
        .await
        .map_err(|e| tracing::warn!(error = %e, "k8s client unavailable (list_all_running_pods)"))
        .ok()?;
    let api: kube::api::Api<k8s_openapi::api::core::v1::Pod> =
        kube::api::Api::namespaced(client, &namespace);
    let lp = kube::api::ListParams::default().labels("edgeflow-target");
    let pods = api
        .list(&lp)
        .await
        .map_err(|e| tracing::warn!(error = %e, "failed to list all pods"))
        .ok()?;
    let mut map: std::collections::HashMap<String, Vec<TargetPod>> =
        std::collections::HashMap::new();
    for pod in pods.items {
        let Some(pod_id) = pod.metadata.name.clone() else {
            continue;
        };
        let Some(target_name) = pod
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get("edgeflow-target"))
            .cloned()
        else {
            continue;
        };
        // pod_ip may be absent while the pod is starting; include it anyway.
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
        let health = health_from_pod(&pod);
        map.entry(target_name).or_default().push(TargetPod {
            pod_id,
            address,
            node,
            registered_at,
            health,
        });
    }
    Some(map)
}

/// List all node names in the cluster.
/// Returns an empty vec if the cluster is unreachable.
pub async fn list_nodes() -> Vec<String> {
    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "k8s client unavailable (list_nodes)");
            return vec![];
        }
    };
    let api: Api<k8s_openapi::api::core::v1::Node> = Api::all(client);
    match api.list(&ListParams::default()).await {
        Ok(list) => list
            .items
            .into_iter()
            .filter_map(|n| n.metadata.name)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to list k8s nodes");
            vec![]
        }
    }
}
