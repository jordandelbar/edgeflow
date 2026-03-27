use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Container, ContainerPort, EnvVar, EnvVarSource, HTTPGetAction, ObjectFieldSelector,
    PodSpec, PodTemplateSpec, Probe, ResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use edgeflow_core::ResourceSettings;

/// Sanitise a target name into a valid k8s resource name.
/// k8s names: lowercase alphanumeric + `-`, max 63 chars.
fn k8s_name(target: &str) -> String {
    let sanitized: String = target
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!("edgeflow-inference-{}", &sanitized[..sanitized.len().min(45)])
}

/// Create a k8s Deployment for an inference pod serving `target`.
///
/// `node` pins the pod to a specific node by name (k3s/k3d node names
/// like `k3d-cluster-agent-0`). Pass `None` to let the scheduler decide.
///
/// No-ops gracefully if:
/// - the cluster is unreachable (local dev without k8s)
/// - the Deployment already exists (pod is starting but hasn't registered yet)
pub async fn create_inference_pod(target: &str, node: Option<&str>, resources: &ResourceSettings) {
    let image = std::env::var("EDGEFLOW_INFERENCE_IMAGE")
        .unwrap_or_else(|_| "edgeflow-inference:latest".into());
    let server_url = std::env::var("EDGEFLOW_SERVER_URL")
        .unwrap_or_else(|_| "http://edgeflow-server:5000".into());
    let namespace = std::env::var("EDGEFLOW_NAMESPACE")
        .unwrap_or_else(|_| "default".into());
    let pull_policy = std::env::var("EDGEFLOW_IMAGE_PULL_POLICY")
        .unwrap_or_else(|_| "IfNotPresent".into());

    // Per-target resource settings, falling back to env var defaults.
    let cpu_request = resources.cpu_request.clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_CPU_REQUEST").ok())
        .unwrap_or_else(|| "100m".into());
    let memory_request = resources.memory_request.clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_MEMORY_REQUEST").ok())
        .unwrap_or_else(|| "256Mi".into());
    let memory_limit = resources.memory_limit.clone()
        .or_else(|| std::env::var("EDGEFLOW_INFERENCE_MEMORY_LIMIT").ok())
        .unwrap_or_else(|| "512Mi".into());
    let max_concurrent = resources.max_concurrent
        .or_else(|| std::env::var("EDGEFLOW_MAX_CONCURRENT_INFER").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(8);
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
            replicas: Some(1),
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
                                name: "EDGEFLOW_MAX_CONCURRENT_INFER".to_string(),
                                value: Some(max_concurrent.to_string()),
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
                        ]),
                        resources: Some(ResourceRequirements {
                            requests: Some([
                                ("cpu".into(),    Quantity(cpu_request)),
                                ("memory".into(), Quantity(memory_request)),
                            ].into()),
                            limits: Some([
                                ("memory".into(), Quantity(memory_limit)),
                            ].into()),
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

/// Delete the k8s Deployment for `target`.
/// No-ops gracefully if the cluster is unreachable or the Deployment doesn't exist.
pub async fn delete_inference_pod(target: &str) {
    let namespace = std::env::var("EDGEFLOW_NAMESPACE")
        .unwrap_or_else(|_| "default".into());

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

/// List all node names in the cluster.
/// Returns an empty vec if the cluster is unreachable.
pub async fn list_nodes() -> Vec<String> {
    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let api: Api<k8s_openapi::api::core::v1::Node> = Api::all(client);
    match api.list(&ListParams::default()).await {
        Ok(list) => list.items.into_iter()
            .filter_map(|n| n.metadata.name)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to list k8s nodes");
            vec![]
        }
    }
}
