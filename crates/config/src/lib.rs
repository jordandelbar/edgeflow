//! Typed configuration for edgeflow services.
//!
//! Each service has a `*Config` struct with a `from_env()` constructor that
//! reads environment variables, applies defaults, and returns a clear error
//! for any required variable that is missing.

use std::path::PathBuf;

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Runtime environment of the service, read from `EDGEFLOW_ENV`.
///
/// Used by the telemetry crate to select the log format (JSON in production,
/// pretty-print in development).
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn from_env() -> Self {
        match std::env::var("EDGEFLOW_ENV").as_deref() {
            Ok(v) if v.eq_ignore_ascii_case("production") => Self::Production,
            _ => Self::Development,
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }
}

// ---------------------------------------------------------------------------
// Inference pod config
// ---------------------------------------------------------------------------

pub struct InferenceConfig {
    /// URL of the edgeflow-server (e.g. `http://edgeflow-server:5000`).
    pub server_url: String,
    /// Deployment target name this pod serves (e.g. `iris-inference`).
    pub target: String,
    /// Address the HTTP inference server binds to. Default: `0.0.0.0:8080`.
    pub infer_addr: String,
    /// Advertised address used when registering with the server.
    pub self_address: String,
    /// k8s node name, if available.
    pub node_name: Option<String>,
    /// Pod identity used for registration. Falls back to `target`.
    pub pod_id: String,
    /// Number of model sessions to keep warm. Default: `1`.
    pub sessions: usize,
    /// Max concurrent inference requests. Default: `sessions`.
    pub max_concurrent: usize,
    /// External MQTT broker URL. If absent the server's embedded broker is used.
    pub mqtt_url: Option<String>,
}

impl InferenceConfig {
    pub fn from_env() -> Result<Self> {
        let server_url = std::env::var("EDGEFLOW_SERVER")
            .context("EDGEFLOW_SERVER is required (e.g. http://edgeflow-server:5000)")?;
        let target = std::env::var("EDGEFLOW_TARGET")
            .context("EDGEFLOW_TARGET is required (e.g. iris-inference)")?;
        let infer_addr =
            std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

        let pod_id = std::env::var("EDGEFLOW_POD_NAME").unwrap_or_else(|_| target.clone());

        let pod_ip = std::env::var("EDGEFLOW_POD_IP").unwrap_or_else(|_| {
            infer_addr
                .split(':')
                .next()
                .unwrap_or("127.0.0.1")
                .replace("0.0.0.0", "127.0.0.1")
        });
        let port = infer_addr.split(':').last().unwrap_or("8080");
        let self_address = format!("http://{pod_ip}:{port}");

        let sessions = std::env::var("EDGEFLOW_SESSIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1usize);
        let max_concurrent = std::env::var("EDGEFLOW_MAX_CONCURRENT_INFER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(sessions);

        let node_name = std::env::var("EDGEFLOW_NODE_NAME").ok();
        let mqtt_url = std::env::var("EDGEFLOW_MQTT_URL").ok();

        Ok(Self {
            server_url,
            target,
            infer_addr,
            self_address,
            node_name,
            pod_id,
            sessions,
            max_concurrent,
            mqtt_url,
        })
    }
}

// ---------------------------------------------------------------------------
// Server config
// ---------------------------------------------------------------------------

pub struct ServerConfig {
    /// Root directory for persistent data. Default: `./data`.
    pub data_dir: PathBuf,
    /// Address the HTTP server binds to. Default: `0.0.0.0:5000`.
    pub addr: String,
    /// Directory serving the UI static files. Default: `./static`.
    pub static_dir: String,
    /// External MQTT broker URL. If absent an embedded broker is started.
    pub mqtt_url: Option<String>,
    /// Port for the embedded MQTT broker (or the external one if URL is set). Default: `1883`.
    pub mqtt_port: u16,
    /// Seconds before a deployment stuck in `deploying`/`upgrading` is marked failed. Default: `300`.
    pub deployment_timeout_secs: i64,
    /// Prometheus base URL for the live-stats endpoint. Optional — if absent the
    /// `/api/v1/targets/:target/stats` endpoint returns 404 and the UI hides stats.
    pub prometheus_url: Option<String>,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        let data_dir =
            PathBuf::from(std::env::var("EDGEFLOW_DATA_DIR").unwrap_or_else(|_| "./data".into()));
        let addr = std::env::var("EDGEFLOW_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".into());
        let static_dir = std::env::var("EDGEFLOW_STATIC_DIR").unwrap_or_else(|_| "./static".into());
        let mqtt_url = std::env::var("EDGEFLOW_MQTT_URL").ok();
        let mqtt_port = std::env::var("EDGEFLOW_MQTT_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1883u16);
        let deployment_timeout_secs = std::env::var("DEPLOYMENT_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300i64);

        let prometheus_url = std::env::var("PROMETHEUS_URL").ok();

        Ok(Self {
            data_dir,
            addr,
            static_dir,
            mqtt_url,
            mqtt_port,
            deployment_timeout_secs,
            prometheus_url,
        })
    }
}
