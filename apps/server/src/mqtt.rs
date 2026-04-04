use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use edgeflow_common::parse_broker_addr;
use rumqttc::{AsyncClient, MqttOptions, QoS};

// ── Publisher ─────────────────────────────────────────────────────────────────

/// Shared MQTT client for publishing commands from the server to inference pods.
pub struct MqttPublisher {
    client: AsyncClient,
}

impl MqttPublisher {
    /// Create a publisher connected to either an external broker (`broker_url`)
    /// or the embedded broker on localhost.  The eventloop runs in a background
    /// task; this function returns immediately.
    pub fn new(broker_url: Option<&str>, mqtt_port: u16) -> Arc<Self> {
        let (host, port) = broker_url
            .map(parse_broker_addr)
            .unwrap_or_else(|| ("localhost".to_string(), mqtt_port));

        let mut options = MqttOptions::new("edgeflow-server-pub", &host, port);
        options.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = AsyncClient::new(options, 64);

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("mqtt publisher: {e}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        tracing::info!(broker = %host, port, "mqtt: publisher ready");
        Arc::new(Self { client })
    }

    /// Publish an upgrade command to all pods registered for `target`.
    pub async fn publish_upgrade(
        &self,
        target: &str,
        run_id: &str,
        deployment_id: &str,
        sessions: usize,
    ) -> Result<()> {
        let topic = format!("edgeflow/targets/{target}/commands");
        let payload = serde_json::json!({
            "command":       "upgrade",
            "run_id":        run_id,
            "deployment_id": deployment_id,
            "sessions":      sessions,
        })
        .to_string();

        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload.as_bytes())
            .await
            .context("mqtt publish_upgrade failed")?;

        Ok(())
    }
}

/// Start an embedded rumqttd broker on `port` in a dedicated OS thread.
/// Returns immediately; the broker keeps running until the process exits.
pub fn start_embedded_broker(port: u16) -> Result<()> {
    let config_toml = format!(
        r#"
id = 0

[router]
max_segment_size = 1048576
max_segment_count = 2
max_connections = 128
max_outgoing_packet_count = 16
max_schedule_iterations = 100
topic_aliases = 0
dynamic_filters = true

[v4.1]
name = "v4-1"
listen = "0.0.0.0:{port}"
next_connection_delay_ms = 1

[v4.1.connections]
connection_timeout_ms = 5000
max_payload_size = 4096
max_inflight_count = 16
max_inflight_size = 4096
dynamic_filters = false
"#
    );

    let config: rumqttd::Config = toml::from_str(&config_toml).context("invalid rumqttd config")?;

    std::thread::Builder::new()
        .name("rumqttd".to_string())
        .spawn(move || {
            let mut broker = rumqttd::Broker::new(config);
            if let Err(e) = broker.start() {
                tracing::error!("rumqttd broker exited: {e:#}");
            }
        })
        .context("failed to spawn rumqttd thread")?;

    Ok(())
}
