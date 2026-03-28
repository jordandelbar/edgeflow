pub mod sqlite;

use edgeflow_core::*;
use anyhow::Result;

/// Unified store trait — implement this for any backend.
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    // Experiments
    async fn create_experiment(&self, name: &str, artifact_location: Option<&str>, tags: Vec<ExperimentTag>) -> Result<Experiment>;
    async fn get_experiment(&self, experiment_id: &str) -> Result<Experiment>;
    async fn get_experiment_by_name(&self, name: &str) -> Result<Experiment>;
    async fn list_experiments(&self, lifecycle_stage: Option<LifecycleStage>) -> Result<Vec<Experiment>>;
    async fn delete_experiment(&self, experiment_id: &str) -> Result<()>;
    async fn restore_experiment(&self, experiment_id: &str) -> Result<()>;
    async fn update_experiment(&self, experiment_id: &str, new_name: &str) -> Result<()>;
    async fn set_experiment_tag(&self, experiment_id: &str, key: &str, value: &str) -> Result<()>;

    // Runs
    async fn create_run(&self, experiment_id: &str, run_name: Option<&str>, start_time: Option<i64>, tags: Vec<RunTag>) -> Result<Run>;
    async fn get_run(&self, run_id: &str) -> Result<Run>;
    async fn update_run(&self, run_id: &str, status: RunStatus, end_time: Option<i64>, run_name: Option<&str>) -> Result<RunInfo>;
    async fn delete_run(&self, run_id: &str) -> Result<()>;
    async fn restore_run(&self, run_id: &str) -> Result<()>;
    async fn search_runs(&self, experiment_ids: Vec<String>, filter: Option<&str>, max_results: i64) -> Result<Vec<Run>>;

    // Metrics / Params / Tags
    async fn log_metric(&self, run_id: &str, metric: Metric) -> Result<()>;
    async fn log_batch(&self, run_id: &str, metrics: Vec<Metric>, params: Vec<Param>, tags: Vec<RunTag>) -> Result<()>;
    async fn log_param(&self, run_id: &str, key: &str, value: &str) -> Result<()>;
    async fn set_tag(&self, run_id: &str, key: &str, value: &str) -> Result<()>;
    async fn get_metric_history(&self, run_id: &str, metric_key: &str) -> Result<Vec<Metric>>;

    // Artifacts
    async fn list_artifacts(&self, run_id: &str, path: Option<&str>) -> Result<Vec<FileInfo>>;
    async fn artifact_root(&self, run_id: &str) -> Result<std::path::PathBuf>;

    // Deployments
    async fn create_deployment(&self, run_id: &str, target: &str) -> Result<Deployment>;
    async fn get_deployment(&self, deployment_id: &str) -> Result<Deployment>;
    async fn get_latest_deployment(&self, target: &str) -> Result<Deployment>;
    async fn list_deployments(&self, target: Option<&str>) -> Result<Vec<Deployment>>;
    async fn update_deployment_state(&self, deployment_id: &str, state: DeploymentState) -> Result<()>;
    async fn get_pending_deployment_for_target(&self, target: &str) -> Result<Option<Deployment>>;
    async fn supersede_previous_deployments(&self, target: &str, except_id: &str) -> Result<()>;
    async fn get_stale_deployments(&self, states: &[&str], older_than_ms: i64) -> Result<Vec<Deployment>>;

    // Targets
    async fn register_target(&self, target: &str, address: &str, pod_name: Option<&str>) -> Result<Target>;
    async fn heartbeat_target(&self, target: &str) -> Result<()>;
    async fn set_target_model(&self, target: &str, run_id: &str, loaded_at: &str) -> Result<()>;
    async fn store_target_resources(&self, target: &str, node: Option<&str>, resources: &ResourceSettings) -> Result<()>;
    async fn get_target(&self, target: &str) -> Result<Option<Target>>;
    async fn list_targets(&self) -> Result<Vec<Target>>;
    async fn delete_target(&self, target: &str) -> Result<()>;
}
