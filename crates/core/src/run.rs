use crate::metric::Metric;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub info: RunInfo,
    pub data: RunData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    pub run_id: String,
    pub run_uuid: String, // same as run_id, MLflow legacy field
    pub experiment_id: String,
    pub run_name: Option<String>,
    pub status: RunStatus,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub artifact_uri: String,
    pub lifecycle_stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunData {
    pub metrics: Vec<Metric>,
    pub params: Vec<Param>,
    pub tags: Vec<RunTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    Running,
    Scheduled,
    Finished,
    Failed,
    Killed,
}
