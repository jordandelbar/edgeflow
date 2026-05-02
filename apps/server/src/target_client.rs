use anyhow::{Context, Result};

/// HTTP client for communicating with a running inference pod.
pub struct TargetClient<'a> {
    http: &'a reqwest::Client,
    address: &'a str,
}

impl<'a> TargetClient<'a> {
    pub fn new(http: &'a reqwest::Client, address: &'a str) -> Self {
        Self { http, address }
    }

    /// `GET /health` - returns the pod's health JSON.
    pub async fn health(&self) -> Result<serde_json::Value> {
        self.http
            .get(format!("{}/health", self.address))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("pod unreachable"))?
            .json()
            .await
            .context("failed to parse health response")
    }

    /// `GET /schema` - returns the model's schema JSON.
    pub async fn schema(&self) -> Result<serde_json::Value> {
        let resp = self
            .http
            .get(format!("{}/schema", self.address))
            .send()
            .await
            .context("failed to reach inference pod")?;
        anyhow::ensure!(resp.status().is_success(), "schema not available");
        resp.json().await.context("failed to parse schema response")
    }

    /// `POST /infer` with a caller-chosen Content-Type. The playground
    /// proxy passes the original Content-Type through so a JSON-object
    /// request (Named mode) or an image upload (`image/jpeg`) reaches the
    /// pod with the right header instead of being lied about as
    /// `application/octet-stream`.
    pub async fn infer_with_content_type(
        &self,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<Vec<u8>> {
        let resp = self
            .http
            .post(format!("{}/infer", self.address))
            .header("content-type", content_type)
            .body(body)
            .send()
            .await
            .context("failed to reach inference pod")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let msg = resp.text().await.unwrap_or_default();
            anyhow::bail!("inference pod returned {status}: {msg}");
        }
        Ok(resp
            .bytes()
            .await
            .context("failed to read inference response")?
            .to_vec())
    }

    /// `POST /upgrade` - instructs the pod to load a new run.
    ///
    /// Returns `true` if the pod accepted the request (2xx / 202), `false` if
    /// it responded with a non-success status.  Returns `Err` only if the pod
    /// was unreachable.
    pub async fn upgrade(
        &self,
        run_id: &str,
        deployment_id: &str,
        sessions: usize,
    ) -> Result<bool> {
        let body = serde_json::json!({
            "run_id":        run_id,
            "deployment_id": deployment_id,
            "sessions":      sessions,
        });
        let resp = self
            .http
            .post(format!("{}/upgrade", self.address))
            .json(&body)
            .send()
            .await
            .context("failed to reach inference pod for upgrade")?;
        Ok(resp.status().is_success() || resp.status().as_u16() == 202)
    }
}
