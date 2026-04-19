use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    Pending,
    Deploying,
    Upgrading,
    Deployed,
    Failed,
    Superseded,
}

impl DeploymentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentState::Pending => "pending",
            DeploymentState::Deploying => "deploying",
            DeploymentState::Upgrading => "upgrading",
            DeploymentState::Deployed => "deployed",
            DeploymentState::Failed => "failed",
            DeploymentState::Superseded => "superseded",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "deploying" => DeploymentState::Deploying,
            "upgrading" => DeploymentState::Upgrading,
            "deployed" => DeploymentState::Deployed,
            "failed" => DeploymentState::Failed,
            "superseded" => DeploymentState::Superseded,
            _ => DeploymentState::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub deployment_id: String,
    pub target: String,
    pub run_id: String,
    pub model_name: Option<String>,
    pub model_version: Option<String>,
    pub created_at: i64,
    pub state: DeploymentState,
}

/// Edgeflow-owned inference settings, persisted in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceSettings {
    /// Number of ORT sessions to keep in the pool (true parallelism).
    /// Defaults to 1 on the inference pod if not set.
    pub sessions: Option<i64>,
    /// Maximum in-flight requests before returning 429.
    /// Defaults to `sessions` on the inference pod if not set.
    pub max_concurrent: Option<i64>,
}

/// Pod placement strategy. `None` means no affinity rule (scheduler decides freely).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// Anti-affinity: prefer scheduling each replica on a different node.
    Spread,
    /// Affinity: prefer scheduling all replicas on the same node.
    Pack,
}

/// k8s-owned infrastructure settings. Never persisted by edgeflow;
/// read from / written to the k8s Deployment spec directly.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfraSettings {
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub memory_limit: Option<String>,
    /// Number of inference pod replicas in the k8s Deployment.
    pub replicas: Option<i32>,
    /// Pod placement strategy. `None` means no affinity rule (scheduler decides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// k8s nodeSelector labels - scheduler picks any node matching all labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_selector: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetHealth {
    /// k8s status unknown (cluster unreachable or pod not yet scheduled).
    Unknown,
    /// Pod is Running and its readiness probe is passing.
    Healthy,
    /// Pod is Running but not yet ready (starting up or failing probe).
    Stale,
    /// Pod is in Failed or Unknown phase.
    Unhealthy,
}

/// A single running inference pod, populated from k8s pod status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetPod {
    pub pod_id: String,
    /// `http://{pod_ip}:8080` derived from k8s pod status.
    pub address: String,
    pub node: Option<String>,
    pub registered_at: i64,
    pub health: TargetHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub target: String,
    pub registered_at: i64,
    /// Edgeflow-owned settings (sessions, max_concurrent) from SQLite.
    pub resources: ResourceSettings,
    /// k8s-owned infrastructure settings read from the k8s Deployment spec.
    /// None when k8s is unreachable or the Deployment has not been created yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infra: Option<InfraSettings>,
    pub current_run_id: Option<String>,
    pub model_loaded_at: Option<String>,
    pub pods: Vec<TargetPod>,
    /// Best health across all pods; Unknown when k8s is unreachable.
    pub health: TargetHealth,
    /// Convenience alias: node of the first pod, if any.
    pub node: Option<String>,
}
