//! Typed configuration for edgeflow services.
//!
//! Each service has a `*Config` struct with a `from_env()` constructor that
//! reads environment variables, applies defaults, and returns a clear error
//! for any required variable that is missing.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

// Environment

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

// Inference pod config
pub struct InferenceConfig {
    /// URL of the edgeflow-server.
    pub server_url: String,
    /// Deployment target name this pod serves.
    pub target: Arc<str>,
    /// Address the HTTP inference server binds to. Default: `0.0.0.0:8080`.
    pub infer_addr: String,
    /// Advertised address used when registering with the server.
    pub self_address: Arc<str>,
    /// k8s node name, if available.
    pub node_name: Option<Arc<str>>,
    /// Pod identity used for registration. Falls back to `target`.
    pub pod_id: Arc<str>,
    /// Pipeline pool size (pre/post WASM contexts per request slot). The
    /// ORT session is shared across slots, so this does NOT scale model
    /// weights. Default: `1`.
    pub sessions: usize,
    /// Max concurrent inference requests. Default: `sessions`.
    pub max_concurrent: usize,
    /// External MQTT broker URL. If absent the server's embedded broker is used.
    pub mqtt_url: Option<String>,
    /// When true, subscribe to `edgeflow/targets/+/commands` (wildcard) instead
    /// of the target-specific topic. Used in compose mode so the pod picks up
    /// upgrade commands for any target name the user deploys to.
    pub dynamic_topic: bool,
}

impl InferenceConfig {
    pub fn from_env() -> Result<Self> {
        let server_url = std::env::var("EDGEFLOW_SERVER")
            .context("EDGEFLOW_SERVER is required (e.g. http://edgeflow-server:5000)")?;
        let target = std::env::var("EDGEFLOW_TARGET")
            .context("EDGEFLOW_TARGET is required (e.g. iris-inference)")?;
        let infer_addr =
            std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

        let pod_id = std::env::var("EDGEFLOW_POD_NAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| target.clone());

        let pod_ip = std::env::var("EDGEFLOW_POD_IP").unwrap_or_else(|_| {
            infer_addr
                .split(':')
                .next()
                .unwrap_or("127.0.0.1")
                .replace("0.0.0.0", "127.0.0.1")
        });
        let port = infer_addr.split(':').next_back().unwrap_or("8080");
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
        let dynamic_topic = std::env::var("EDGEFLOW_POD_DYNAMIC_TOPIC")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        Ok(Self {
            server_url,
            target: target.into(),
            infer_addr,
            self_address: self_address.into(),
            node_name: node_name.map(Into::into),
            pod_id: pod_id.into(),
            sessions,
            max_concurrent,
            mqtt_url,
            dynamic_topic,
        })
    }
}

/// Which substrate runs inference pods. Defaults to `K8s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorKind {
    K8s,
    Compose,
}

/// CORS policy for the server's HTTP surface. Defaults to `Disabled` -
/// the docker-compose demo serves the UI from the same origin, so CORS
/// is unnecessary, and a permissive default would be needlessly open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsPolicy {
    /// No CORS layer. The browser's Same-Origin Policy applies; same-origin
    /// requests work, cross-origin browser requests are blocked by the
    /// browser. curl and server-to-server clients are unaffected.
    Disabled,
    /// Any origin, any method, any header. Set
    /// `EDGEFLOW_CORS_ALLOW_ORIGINS=*` to opt in.
    Any,
    /// Exact-match allowlist of origins, e.g.
    /// `EDGEFLOW_CORS_ALLOW_ORIGINS=https://dashboard.example.com,https://other.example.com`.
    Allowlist(Vec<String>),
}

impl CorsPolicy {
    fn from_env() -> Self {
        Self::parse(std::env::var("EDGEFLOW_CORS_ALLOW_ORIGINS").ok().as_deref())
    }

    /// Pure parser for the `EDGEFLOW_CORS_ALLOW_ORIGINS` value. `None`
    /// represents an unset variable.
    fn parse(value: Option<&str>) -> Self {
        match value {
            None => Self::Disabled,
            Some(s) if s.trim().is_empty() => Self::Disabled,
            Some(s) if s.trim() == "*" => Self::Any,
            Some(s) => Self::Allowlist(
                s.split(',')
                    .map(|o| o.trim().to_string())
                    .filter(|o| !o.is_empty())
                    .collect(),
            ),
        }
    }
}

// Server config
pub struct ServerConfig {
    /// Root directory for persistent data.
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
    /// Prometheus base URL for the live-stats endpoint. Optional, if absent the
    /// `/api/v1/targets/:target/stats` endpoint returns 404 and the UI hides stats.
    pub prometheus_url: Option<String>,
    /// Selects the inference-pod runtime. Defaults to `K8s`. Set
    /// `EDGEFLOW_ORCHESTRATOR=compose` for the docker-compose demo path.
    pub orchestrator: OrchestratorKind,
    /// URL the server uses to reach the compose inference container,
    /// e.g. `http://inference:8080`. Required when `orchestrator == Compose`.
    pub compose_inference_url: Option<String>,
    /// CORS policy for the public HTTP surface. Defaults to `Disabled`.
    pub cors: CorsPolicy,
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

        let orchestrator = match std::env::var("EDGEFLOW_ORCHESTRATOR").ok().as_deref() {
            Some("compose") => OrchestratorKind::Compose,
            _ => OrchestratorKind::K8s,
        };
        let compose_inference_url = std::env::var("EDGEFLOW_COMPOSE_INFERENCE_URL").ok();

        if orchestrator == OrchestratorKind::Compose && compose_inference_url.is_none() {
            anyhow::bail!("EDGEFLOW_ORCHESTRATOR=compose requires EDGEFLOW_COMPOSE_INFERENCE_URL");
        }

        Ok(Self {
            data_dir,
            addr,
            static_dir,
            mqtt_url,
            mqtt_port,
            deployment_timeout_secs,
            prometheus_url,
            orchestrator,
            compose_inference_url,
            cors: CorsPolicy::from_env(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_unset_is_disabled() {
        assert_eq!(CorsPolicy::parse(None), CorsPolicy::Disabled);
    }

    #[test]
    fn cors_empty_is_disabled() {
        assert_eq!(CorsPolicy::parse(Some("")), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse(Some("   ")), CorsPolicy::Disabled);
    }

    #[test]
    fn cors_wildcard_is_any() {
        assert_eq!(CorsPolicy::parse(Some("*")), CorsPolicy::Any);
        assert_eq!(CorsPolicy::parse(Some("  *  ")), CorsPolicy::Any);
    }

    #[test]
    fn cors_single_origin_is_allowlist() {
        assert_eq!(
            CorsPolicy::parse(Some("https://example.com")),
            CorsPolicy::Allowlist(vec!["https://example.com".into()]),
        );
    }

    #[test]
    fn cors_csv_origins_are_trimmed() {
        assert_eq!(
            CorsPolicy::parse(Some("https://a.com, https://b.com ,https://c.com")),
            CorsPolicy::Allowlist(vec![
                "https://a.com".into(),
                "https://b.com".into(),
                "https://c.com".into(),
            ]),
        );
    }

    #[test]
    fn cors_empty_csv_entries_are_dropped() {
        assert_eq!(
            CorsPolicy::parse(Some("https://a.com,,https://b.com,")),
            CorsPolicy::Allowlist(vec!["https://a.com".into(), "https://b.com".into()]),
        );
    }

    #[test]
    fn cors_wildcard_inside_csv_is_treated_as_origin() {
        // An exact-match allowlist of literal "*" is nonsense as a real origin
        // but the parser keeps it; only a bare "*" (after trim) means "Any".
        assert_eq!(
            CorsPolicy::parse(Some("*,https://a.com")),
            CorsPolicy::Allowlist(vec!["*".into(), "https://a.com".into()]),
        );
    }
}
