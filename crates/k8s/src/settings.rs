//! Apply env-var overrides and hardcoded defaults to user-supplied settings.

use edgeflow_core::{InfraSettings, ResourceSettings};

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
