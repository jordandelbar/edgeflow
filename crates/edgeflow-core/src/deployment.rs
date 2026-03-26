use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    Pending,
    Deploying,
    Upgrading,
    Healthy,
    Failed,
    Superseded,
}

impl DeploymentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentState::Pending => "pending",
            DeploymentState::Deploying => "deploying",
            DeploymentState::Upgrading => "upgrading",
            DeploymentState::Healthy => "healthy",
            DeploymentState::Failed => "failed",
            DeploymentState::Superseded => "superseded",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "deploying" => DeploymentState::Deploying,
            "upgrading" => DeploymentState::Upgrading,
            "healthy" => DeploymentState::Healthy,
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
    pub created_at: i64,
    pub state: DeploymentState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub target: String,
    pub address: String,
    pub pod_name: Option<String>,
    pub registered_at: i64,
}
