//! Create / read / update / delete inference pod Deployments.

use std::collections::BTreeMap;

use edgeflow_core::{InfraSettings, Placement, ResourceSettings};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Container, ContainerPort, EnvVar, EnvVarSource, HTTPGetAction, ObjectFieldSelector, PodSpec,
    PodTemplateSpec, Probe, ResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, DeleteParams, PostParams};

use crate::client::{client, namespace};
use crate::naming::{k8s_name, placement_affinity, TARGET_LABEL};
use crate::settings::{resolve_infra, resolve_resources};

/// Read infrastructure settings for `target` from the k8s Deployment spec.
/// Returns `None` when k8s is unreachable or the Deployment doesn't exist.
pub async fn get_inference_pod_infra(target: &str) -> Option<InfraSettings> {
    let client = client("get_inference_pod_infra").await?;
    let api: Api<Deployment> = Api::namespaced(client, &namespace());
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
        .map(|m| m.into_iter().collect::<BTreeMap<_, _>>());

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
    let namespace = namespace();
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
    // No CPU limit - throttling degrades inference latency more than an OOM would.

    let Some(client) = client("create_inference_pod").await else {
        tracing::warn!(
            target = %target,
            "k8s client unavailable - start the inference pod manually with \
             EDGEFLOW_SERVER={server_url} EDGEFLOW_TARGET={target}"
        );
        return;
    };

    let name = k8s_name(target);
    let mut labels = BTreeMap::new();
    labels.insert("app".to_string(), name.clone());
    labels.insert(TARGET_LABEL.to_string(), target.to_string());

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
                            initial_delay_seconds: Some(0),
                            period_seconds: Some(1),
                            failure_threshold: Some(300),
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
                "inference deployment already exists - waiting for pod to register"
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
/// Either can be `None` - only the provided parts are changed.
///
/// Returns `true` if the update was accepted, `false` if the cluster is
/// unreachable, the Deployment doesn't exist, or the update failed.
pub async fn patch_inference_pod_resources(
    target: &str,
    resources: Option<&ResourceSettings>,
    infra: Option<&InfraSettings>,
) -> bool {
    let Some(client) = client("patch_inference_pod_resources").await else {
        return false;
    };

    let name = k8s_name(target);
    let api: Api<Deployment> = Api::namespaced(client, &namespace());

    // GET the current Deployment so we preserve all existing fields and have
    // the correct resourceVersion for optimistic concurrency.
    let mut deployment = match api.get(&name).await {
        Ok(d) => d,
        Err(kube::Error::Api(e)) if e.code == 404 => {
            tracing::warn!(
                target = %target,
                name = %name,
                "inference deployment not found - cannot update resources (was it created via k8s?)"
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
        .replace(&name, &PostParams::default(), &deployment)
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
    let Some(client) = client("delete_inference_pod").await else {
        tracing::warn!(
            target = %target,
            "k8s client unavailable - delete the inference pod manually"
        );
        return;
    };

    let name = k8s_name(target);
    let api: Api<Deployment> = Api::namespaced(client, &namespace());
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
