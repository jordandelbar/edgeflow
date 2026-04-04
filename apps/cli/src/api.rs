use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct Api {
    client: Client,
    pub base: String,
}

impl Api {
    pub fn new(server: &str) -> Self {
        Self {
            client: Client::new(),
            base: server.trim_end_matches('/').to_string(),
        }
    }

    fn mlflow(&self, path: &str) -> String {
        format!("{}/api/2.0/mlflow{path}", self.base)
    }

    fn v1(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base)
    }

    fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.client
            .get(url)
            .send()?
            .error_for_status()
            .context("request failed")?
            .json::<T>()
            .context("failed to parse response")
    }

    fn get_params<T: DeserializeOwned>(&self, url: &str, params: &[(&str, &str)]) -> Result<T> {
        let mut full_url = reqwest::Url::parse(url).context("invalid URL")?;
        full_url
            .query_pairs_mut()
            .extend_pairs(params.iter().copied());
        self.client
            .get(full_url)
            .send()?
            .error_for_status()
            .context("request failed")?
            .json::<T>()
            .context("failed to parse response")
    }

    fn post<T: DeserializeOwned>(&self, url: &str, body: &Value) -> Result<T> {
        self.client
            .post(url)
            .json(body)
            .send()?
            .error_for_status()
            .context("request failed")?
            .json::<T>()
            .context("failed to parse response")
    }

    fn patch<T: DeserializeOwned>(&self, url: &str, body: &Value) -> Result<T> {
        self.client
            .patch(url)
            .json(body)
            .send()?
            .error_for_status()
            .context("request failed")?
            .json::<T>()
            .context("failed to parse response")
    }

    fn delete(&self, url: &str) -> Result<()> {
        self.client
            .delete(url)
            .send()?
            .error_for_status()
            .context("request failed")?;
        Ok(())
    }

    // ── Experiments ───────────────────────────────────────────────────────────

    pub fn list_experiments(&self) -> Result<Value> {
        self.get(&self.mlflow("/experiments/list"))
    }

    pub fn get_experiment(&self, id: &str) -> Result<Value> {
        self.get_params(&self.mlflow("/experiments/get"), &[("experiment_id", id)])
    }

    pub fn get_experiment_by_name(&self, name: &str) -> Result<Value> {
        self.get_params(
            &self.mlflow("/experiments/get-by-name"),
            &[("experiment_name", name)],
        )
    }

    /// Resolve an experiment by name or ID — tries name first.
    pub fn resolve_experiment(&self, name_or_id: &str) -> Result<Value> {
        self.get_experiment_by_name(name_or_id)
            .or_else(|_| self.get_experiment(name_or_id))
    }

    // ── Runs ──────────────────────────────────────────────────────────────────

    pub fn search_runs(&self, experiment_id: &str) -> Result<Value> {
        self.post(
            &self.mlflow("/runs/search"),
            &serde_json::json!({
                "experiment_ids": [experiment_id],
            }),
        )
    }

    pub fn get_run(&self, run_id: &str) -> Result<Value> {
        self.get_params(&self.mlflow("/runs/get"), &[("run_id", run_id)])
    }

    /// Resolve a full run ID from a prefix by searching across all experiments.
    pub fn resolve_run_id(&self, prefix: &str) -> Result<String> {
        if prefix.len() == 32 {
            return Ok(prefix.to_string());
        }
        let exps = self.list_experiments()?;
        for exp in exps["experiments"].as_array().cloned().unwrap_or_default() {
            let exp_id = exp["experiment_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if let Ok(res) = self.search_runs(&exp_id) {
                for run in res["runs"].as_array().cloned().unwrap_or_default() {
                    let id = run["info"]["run_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if id.starts_with(prefix) {
                        return Ok(id);
                    }
                }
            }
        }
        anyhow::bail!("no run found with prefix '{prefix}'")
    }

    // ── Model Registry ────────────────────────────────────────────────────────

    pub fn list_registered_models(&self) -> Result<Value> {
        self.get(&self.mlflow("/registered-models/list"))
    }

    pub fn list_model_versions(&self, name: &str) -> Result<Value> {
        self.post(
            &self.mlflow("/model-versions/search"),
            &serde_json::json!({
                "filter": format!("name = '{name}'"),
            }),
        )
    }

    pub fn register_model(&self, run_id: &str, name: &str) -> Result<Value> {
        // Create registered model (idempotent).
        let _ = self.post::<Value>(
            &self.mlflow("/registered-models/create"),
            &serde_json::json!({ "name": name }),
        );
        // Create version.
        self.post(
            &self.mlflow("/model-versions/create"),
            &serde_json::json!({
                "name": name,
                "run_id": run_id,
            }),
        )
    }

    pub fn transition_stage(&self, name: &str, version: &str, stage: &str) -> Result<Value> {
        self.post(
            &self.mlflow("/model-versions/transition-stage"),
            &serde_json::json!({
                "name": name,
                "version": version,
                "stage": stage,
            }),
        )
    }

    pub fn delete_registered_model(&self, name: &str) -> Result<Value> {
        self.post(
            &self.mlflow("/registered-models/delete"),
            &serde_json::json!({ "name": name }),
        )
    }

    pub fn delete_model_version(&self, name: &str, version: &str) -> Result<Value> {
        self.post(
            &self.mlflow("/model-versions/delete"),
            &serde_json::json!({
                "name": name,
                "version": version,
            }),
        )
    }

    // ── Deployments ───────────────────────────────────────────────────────────

    pub fn create_deployment(
        &self,
        model_name: &str,
        model_version: &str,
        target: &str,
        sessions: Option<i64>,
        max_concurrent: Option<i64>,
    ) -> Result<Value> {
        self.post(
            &self.v1("/deployments"),
            &serde_json::json!({
                "model_name": model_name,
                "model_version": model_version,
                "target": target,
                "resources": {
                    "sessions": sessions,
                    "max_concurrent": max_concurrent,
                },
            }),
        )
    }

    pub fn list_deployments(&self, target: Option<&str>) -> Result<Value> {
        if let Some(t) = target {
            self.get_params(&self.v1("/deployments"), &[("target", t)])
        } else {
            self.get(&self.v1("/deployments"))
        }
    }

    pub fn get_deployment(&self, id: &str) -> Result<Value> {
        self.get(&self.v1(&format!("/deployments/{id}")))
    }

    pub fn latest_deployment(&self, target: &str) -> Result<Value> {
        self.get_params(&self.v1("/deployments/latest"), &[("target", target)])
    }

    // ── Targets ───────────────────────────────────────────────────────────────

    pub fn list_targets(&self) -> Result<Value> {
        self.get(&self.v1("/targets"))
    }

    pub fn get_target(&self, target: &str) -> Result<Value> {
        self.get(&self.v1(&format!("/targets/{target}")))
    }

    pub fn update_target_resources(
        &self,
        target: &str,
        sessions: Option<i64>,
        max_concurrent: Option<i64>,
        cpu_request: Option<&str>,
        memory_request: Option<&str>,
        memory_limit: Option<&str>,
        replicas: Option<i64>,
        placement: Option<&str>,
    ) -> Result<Value> {
        self.patch(
            &self.v1(&format!("/targets/{target}/resources")),
            &serde_json::json!({
                "resources": {
                    "sessions":       sessions,
                    "max_concurrent": max_concurrent,
                },
                "infra": {
                    "cpu_request":    cpu_request,
                    "memory_request": memory_request,
                    "memory_limit":   memory_limit,
                    "replicas":       replicas,
                    "placement":      placement,
                },
            }),
        )
    }

    pub fn teardown_target(&self, target: &str) -> Result<()> {
        self.delete(&self.v1(&format!("/targets/{target}")))
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────

    pub fn list_nodes(&self) -> Result<Value> {
        self.get(&self.v1("/nodes"))
    }
}
