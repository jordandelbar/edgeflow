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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceSettings {
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub memory_limit: Option<String>,
    /// Number of ORT sessions to keep in the pool (true parallelism).
    /// Defaults to 1 on the inference pod if not set.
    pub sessions: Option<i64>,
    /// Maximum in-flight requests before returning 429.
    /// Defaults to `sessions` on the inference pod if not set.
    pub max_concurrent: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetHealth {
    /// No heartbeat ever recorded.
    Unknown,
    /// Last heartbeat within 2× the 30 s interval.
    Healthy,
    /// Heartbeat overdue but recent enough that it may recover (60 s – 5 min).
    Stale,
    /// No heartbeat for > 5 min — pod is almost certainly down.
    Unhealthy,
}

impl TargetHealth {
    pub fn from_last_seen(last_seen: Option<i64>) -> Self {
        let Some(ts) = last_seen else {
            return Self::Unknown;
        };
        let age_secs = (chrono::Utc::now().timestamp_millis() - ts).max(0) / 1000;
        match age_secs {
            0..=59 => Self::Healthy,
            60..=299 => Self::Stale,
            _ => Self::Unhealthy,
        }
    }

    /// Best health across a set of pods (Healthy > Stale > Unhealthy > Unknown).
    pub fn aggregate(pods: &[TargetPod]) -> Self {
        pods.iter()
            .map(|p| p.health.clone())
            .max_by_key(|h| match h {
                Self::Healthy => 3,
                Self::Stale => 2,
                Self::Unhealthy => 1,
                Self::Unknown => 0,
            })
            .unwrap_or(Self::Unknown)
    }
}

/// A single running inference pod registered with the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetPod {
    pub pod_id: String,
    pub address: String,
    pub node: Option<String>,
    pub registered_at: i64,
    pub last_seen: Option<i64>,
    pub health: TargetHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub target: String,
    pub registered_at: i64,
    pub resources: ResourceSettings,
    pub current_run_id: Option<String>,
    pub model_loaded_at: Option<String>,
    pub pods: Vec<TargetPod>,
    // Aggregated from pods for API backwards compatibility.
    pub health: TargetHealth,
    pub node: Option<String>,
    pub last_seen: Option<i64>,
}
