use anyhow::Result;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::deployment::DeployInstruction;

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

/// MQTT client for a single inference pod.
///
/// Subscribes to upgrade commands for its target and forwards them via a
/// `mpsc::Receiver<DeployInstruction>` so `main` can drive `load_and_swap`.
pub struct MqttPodClient {
    _client: AsyncClient,
}

impl MqttPodClient {
    pub fn new(
        broker_url: &str,
        target: &str,
        pod_id: &str,
    ) -> Result<(Self, mpsc::Receiver<DeployInstruction>)> {
        let (host, port) = parse_broker_addr(broker_url);
        let client_id = format!("edgeflow-inference-{pod_id}");

        let mut options = MqttOptions::new(&client_id, &host, port);
        options.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = AsyncClient::new(options, 16);
        let (cmd_tx, cmd_rx) = mpsc::channel::<DeployInstruction>(16);

        let cmd_topic = format!("edgeflow/targets/{target}/commands");
        let client_sub = client.clone();

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        // Re-subscribe after every (re)connect — clean session
                        // does not persist subscriptions across reconnects.
                        if let Err(e) = client_sub.subscribe(&cmd_topic, QoS::AtLeastOnce).await {
                            tracing::warn!("mqtt: failed to subscribe to commands: {e}");
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(p))) if p.topic == cmd_topic => {
                        match serde_json::from_slice::<serde_json::Value>(&p.payload) {
                            Ok(v) if v["command"].as_str() == Some("upgrade") => {
                                let run_id = v["run_id"].as_str().unwrap_or_default().to_string();
                                let deployment_id =
                                    v["deployment_id"].as_str().unwrap_or_default().to_string();
                                let sessions = v["sessions"].as_u64().unwrap_or(1) as usize;
                                let instr = DeployInstruction {
                                    run_id,
                                    deployment_id,
                                    sessions,
                                };
                                if cmd_tx.send(instr).await.is_err() {
                                    // Receiver dropped — main loop is gone, stop.
                                    return;
                                }
                            }
                            Ok(_) => {} // unknown command, ignore
                            Err(e) => {
                                tracing::warn!("mqtt: malformed command payload: {e}");
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("mqtt eventloop: {e}; reconnecting");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        tracing::info!(broker = %host, port, "mqtt: pod client ready");

        Ok((Self { _client: client }, cmd_rx))
    }
}
