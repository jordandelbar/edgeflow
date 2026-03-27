use anyhow::{Context, Result};

pub struct EdgeflowClient {
    server: String,
    client: reqwest::Client,
}

impl EdgeflowClient {
    pub fn new(server: &str) -> Self {
        Self {
            server: server.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Register this pod as the active inference server for `target`.
    pub async fn register_target(&self, target: &str, address: &str) -> Result<()> {
        let url = format!("{}/api/v1/targets/register", self.server);
        self.client
            .post(&url)
            .json(&serde_json::json!({ "target": target, "address": address }))
            .send()
            .await
            .context("failed to register target with edgeflow-server")?
            .error_for_status()
            .context("server rejected target registration")?;
        Ok(())
    }

    /// Confirm the outcome of a deployment to the server.
    pub async fn confirm_deployment(
        &self,
        deployment_id: &str,
        status: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/api/v1/deployments/{}/confirm", self.server, deployment_id);
        let mut body = serde_json::json!({ "status": status });
        if let Some(r) = reason {
            body["reason"] = serde_json::Value::String(r.to_string());
        }
        self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("failed to confirm deployment")?
            .error_for_status()
            .context("server rejected deployment confirmation")?;
        Ok(())
    }

    /// Kept for backwards-compat / dev polling.
    #[allow(dead_code)]
    pub async fn latest_run_id(&self, target: &str) -> Result<String> {
        let url = format!("{}/api/v1/deployments/latest?target={}", self.server, target);
        let resp: serde_json::Value = self.client
            .get(&url)
            .send()
            .await
            .context("failed to reach edgeflow-server")?
            .error_for_status()
            .context("no deployment found for target")?
            .json()
            .await?;

        resp["deployment"]["run_id"]
            .as_str()
            .map(|s| s.to_string())
            .context("deployment response missing run_id")
    }

    pub async fn download_artifact(&self, run_id: &str, path: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/2.0/mlflow/artifacts/get-artifact?run_id={}&path={}",
            self.server, run_id, path
        );
        let bytes = self.client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to download artifact {path}"))?
            .error_for_status()
            .with_context(|| format!("artifact not found: {path}"))?
            .bytes()
            .await?;

        Ok(bytes.to_vec())
    }
}
