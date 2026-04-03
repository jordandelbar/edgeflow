use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, QoS};
use std::time::Duration;

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

/// Lightweight MQTT heartbeat publisher.
///
/// The eventloop runs in a background Tokio task.  Call [`MqttHeartbeat::beat`]
/// on each heartbeat interval; it queues a QoS-0 publish which the eventloop
/// sends on its next iteration.
pub struct MqttHeartbeat {
    client: AsyncClient,
    topic: String,
}

impl MqttHeartbeat {
    /// Connect to the broker at `broker_url` and prepare to publish heartbeats
    /// for `(target, pod_id)`.
    pub fn new(broker_url: &str, target: &str, pod_id: &str) -> Result<Self> {
        let (host, port) = parse_broker_addr(broker_url);
        let client_id = format!("edgeflow-inference-{pod_id}");

        let mut options = MqttOptions::new(&client_id, &host, port);
        options.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = AsyncClient::new(options, 16);

        // Drive the eventloop in the background; log and continue on errors
        // so transient broker restarts don't kill the inference pod.
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(_) | Event::Outgoing(_)) => {}
                    Err(e) => {
                        tracing::warn!("mqtt eventloop: {e}; reconnecting");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let topic = format!("edgeflow/targets/{target}/pods/{pod_id}/heartbeat");
        tracing::info!(broker = %host, port, topic, "mqtt: heartbeat publisher ready");

        Ok(Self { client, topic })
    }

    /// Publish a heartbeat. QoS 0 — fire and forget.
    pub async fn beat(&self) -> Result<()> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let payload = format!(r#"{{"ts":{ts}}}"#);

        self.client
            .publish(&self.topic, QoS::AtMostOnce, false, payload.as_bytes())
            .await
            .context("mqtt publish failed")?;
        Ok(())
    }
}
