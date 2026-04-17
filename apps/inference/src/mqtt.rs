use anyhow::Result;
use edgeflow_common::parse_broker_addr;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::deployment::DeployInstruction;

/// MQTT client for a single inference pod.
///
/// Subscribes to upgrade commands for its target and forwards them via a
/// `mpsc::Receiver<DeployInstruction>` so `main` can drive `load_and_swap`.
///
/// When `dynamic_topic` is `true` the client subscribes to the wildcard
/// `edgeflow/targets/+/commands` instead of a target-specific topic. This
/// lets the compose demo pod pick up upgrade commands for any target without
/// requiring the user to align env vars at startup.
pub struct MqttPodClient {
    _client: AsyncClient,
}

impl MqttPodClient {
    pub fn new(
        broker_url: &str,
        target: &str,
        pod_id: &str,
        dynamic_topic: bool,
    ) -> Result<(Self, mpsc::Receiver<DeployInstruction>)> {
        let (host, port) = parse_broker_addr(broker_url);
        let client_id = format!("edgeflow-inference-{pod_id}");

        let mut options = MqttOptions::new(&client_id, &host, port);
        options.set_keep_alive(Duration::from_secs(60));
        options.set_clean_session(true);

        // Cap of 1: we only ever have one in-flight deploy command at a time.
        let (client, mut eventloop) = AsyncClient::new(options, 1);
        // Buffer of 1: load_and_swap is serial; a second command can't arrive
        // before the first is consumed in practice (retained topic, one server).
        let (cmd_tx, cmd_rx) = mpsc::channel::<DeployInstruction>(1);

        let sub_topic = if dynamic_topic {
            "edgeflow/targets/+/commands".to_string()
        } else {
            format!("edgeflow/targets/{target}/commands")
        };
        let client_sub = client.clone();

        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        // Re-subscribe after every (re)connect — clean session
                        // does not persist subscriptions across reconnects.
                        if let Err(e) = client_sub.subscribe(&sub_topic, QoS::AtLeastOnce).await {
                            tracing::warn!("mqtt: failed to subscribe to commands: {e}");
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(p)))
                        if p.topic.starts_with("edgeflow/targets/")
                            && p.topic.ends_with("/commands") =>
                    {
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

        tracing::info!(broker = %host, port, dynamic_topic, "mqtt: pod client ready");

        Ok((Self { _client: client }, cmd_rx))
    }
}
