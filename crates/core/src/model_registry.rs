use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredModel {
    pub name: String,
    pub creation_time: i64,
    pub last_updated_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub latest_versions: Vec<ModelVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    pub name: String,
    /// String representation per MLflow spec ("1", "2", …)
    pub version: String,
    pub creation_time: i64,
    pub last_updated_time: i64,
    /// "None" | "Staging" | "Production" | "Archived"
    pub current_stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// "READY" | "PENDING_REGISTRATION" | "FAILED_REGISTRATION"
    pub status: String,
}
