use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio_util::sync::CancellationToken;

use edgeflow_store::sqlite::SqliteStore;
use edgeflow_store::Store as _;

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

/// Parse "mqtt://host:port" (or bare "host:port") into (host, port).
fn parse_broker_addr(url: &str) -> (String, u16) {
    let stripped = url
        .trim_start_matches("mqtt://")
        .trim_start_matches("tcp://");
    if let Some((host, port_str)) = stripped.rsplit_once(':') {
        let port = port_str.parse().unwrap_or(1883);
        (host.to_string(), port)
    } else {
        (stripped.to_string(), 1883)
    }
}

/// Start an embedded rumqttd broker on `port` in a dedicated OS thread.
/// Returns immediately; the broker keeps running until the process exits.
pub fn start_embedded_broker(port: u16) -> Result<()> {
    let config_toml = format!(
        r#"
id = 0

[router]
max_segment_size = 104857600
max_segment_count = 10
max_connections = 10000
max_outgoing_packet_count = 200
max_schedule_iterations = 100
topic_aliases = 0
dynamic_filters = true

[v4.1]
name = "v4-1"
listen = "0.0.0.0:{port}"
next_connection_delay_ms = 1

[v4.1.connections]
connection_timeout_ms = 5000
max_payload_size = 65536
max_inflight_count = 200
max_inflight_size = 102400
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

/// Subscribe to `edgeflow/targets/+/heartbeat` and record each heartbeat in
/// the store.  Reconnects automatically on broker restarts or network errors.
///
/// `broker_url` — if `Some`, connect to an external broker (e.g. Mosquitto);
/// if `None`, connect to the embedded broker on localhost.
pub async fn subscribe_heartbeats(
    broker_url: Option<String>,
    mqtt_port: u16,
    store: Arc<SqliteStore>,
    cancel: CancellationToken,
) {
    let (host, port) = broker_url
        .as_deref()
        .map(parse_broker_addr)
        .unwrap_or_else(|| ("localhost".to_string(), mqtt_port));

    let mut options = MqttOptions::new("edgeflow-server-sub", &host, port);
    options.set_keep_alive(Duration::from_secs(30));

    loop {
        if cancel.is_cancelled() {
            return;
        }

        let (client, mut eventloop) = AsyncClient::new(options.clone(), 64);

        // Queue the SUBSCRIBE — sent once the eventloop connects.
        if let Err(e) = client
            .subscribe("edgeflow/targets/+/pods/+/heartbeat", QoS::AtMostOnce)
            .await
        {
            tracing::warn!("mqtt: failed to queue subscribe: {e}; retrying in 5s");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        tracing::info!(broker = %host, port, "mqtt: subscribing to heartbeats");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,

                event = eventloop.poll() => {
                    match event {
                        Ok(Event::Incoming(Packet::Publish(p))) => {
                            // Topic: "edgeflow/targets/{target}/pods/{pod_id}/heartbeat"
                            let parts: Vec<&str> = p.topic.splitn(6, '/').collect();
                            if parts.len() == 6 {
                                let pod_id = parts[4];
                                if let Err(e) = store.heartbeat_pod(pod_id).await {
                                    tracing::warn!(pod_id, "mqtt: heartbeat store error: {e}");
                                } else {
                                    tracing::debug!(pod_id, "mqtt: heartbeat recorded");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("mqtt: connection error: {e}; reconnecting in 5s");
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            break; // recreate client + eventloop
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
