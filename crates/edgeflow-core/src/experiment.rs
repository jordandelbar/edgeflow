use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub experiment_id: String,
    pub name: String,
    pub artifact_location: String,
    pub lifecycle_stage: LifecycleStage,
    pub creation_time: i64,
    pub last_update_time: i64,
    pub tags: Vec<ExperimentTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleStage {
    Active,
    Deleted,
}
