use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio_util::sync::CancellationToken;

use edgeflow_store::sqlite::SqliteStore;
use edgeflow_store::Store as _;

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
            .subscribe("edgeflow/targets/+/heartbeat", QoS::AtMostOnce)
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
                            // Topic: "edgeflow/targets/{target}/heartbeat"
                            let parts: Vec<&str> = p.topic.splitn(4, '/').collect();
                            if parts.len() == 4 {
                                let target = parts[2];
                                if let Err(e) = store.heartbeat_target(target).await {
                                    tracing::warn!(target, "mqtt: heartbeat store error: {e}");
                                } else {
                                    tracing::debug!(target, "mqtt: heartbeat recorded");
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
