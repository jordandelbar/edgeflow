use crate::mqtt::MqttPublisher;
use edgeflow_store::sqlite::SqliteStore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub artifact_root: PathBuf,
    pub http_client: reqwest::Client,
    /// Present when MQTT is available; used to fan-out upgrade commands.
    pub mqtt_publisher: Option<Arc<MqttPublisher>>,
    /// Base URL of a Prometheus instance. When set, enables the live-stats endpoint.
    pub prometheus_url: Option<String>,
}
