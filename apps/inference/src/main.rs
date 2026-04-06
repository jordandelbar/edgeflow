// Core inference modules live in lib.rs, so tests and benches can import them.
mod client;
mod deployment;
mod mqtt;
mod server;

use anyhow::Result;
use client::EdgeflowClient;
use edgeflow_common::{backoff::retry_forever, shutdown_signal};
use edgeflow_config::InferenceConfig;
use edgeflow_telemetry;
use server::{Metrics, ServerState};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Semaphore;
#[tokio::main]
async fn main() -> Result<()> {
    edgeflow_telemetry::init("edgeflow-inference", "edgeflow_inference=info")?;

    let cancel = shutdown_signal();

    let cfg = InferenceConfig::from_env()?;

    let client = Arc::new(EdgeflowClient::new(&cfg.server_url));

    let state = ServerState {
        active: Arc::new(RwLock::new(None)),
        semaphore: Arc::new(Semaphore::new(cfg.max_concurrent)),
        metrics: Arc::new(Metrics::new(&cfg.target, &cfg.pod_id)),
        client: client.clone(),
        target: cfg.target.clone(),
        sessions: cfg.sessions,
    };

    // Start the HTTP server in the background so we're ready before registering.
    let serve_state = state.clone();
    let serve_addr = cfg.infer_addr.clone();
    let serve_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(serve_state, serve_addr, serve_cancel).await {
            tracing::error!("inference server error: {e:#}");
        }
    });

    // Small pause to let the listener bind before we register.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to MQTT *before* registering so we are already subscribed when
    // the server publishes the pending deployment on registration.
    let mut mqtt_commands: Option<tokio::sync::mpsc::Receiver<deployment::DeployInstruction>> =
        match cfg.mqtt_url.as_deref() {
            Some(url) => match mqtt::MqttPodClient::new(url, &cfg.target, &cfg.pod_id) {
                Ok((_client, rx)) => Some(rx),
                Err(e) => {
                    tracing::warn!("mqtt init failed: {e}");
                    None
                }
            },
            None => None,
        };

    // Give the MQTT event loop a moment to connect and subscribe before we
    // register — the server will publish immediately on registration.
    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!(target = %cfg.target, pod_id = %cfg.pod_id, address = %cfg.self_address, node = ?cfg.node_name, "registering with edgeflow-server");
    retry_forever("register with edgeflow-server", || {
        let client = client.clone();
        let target = cfg.target.clone();
        let pod_id = cfg.pod_id.clone();
        let self_address = cfg.self_address.clone();
        let node = cfg.node_name.clone();
        async move {
            client
                .register_pod(&pod_id, &target, &self_address, node.as_deref())
                .await
        }
    })
    .await;
    tracing::info!("registered");

    // Background task: process upgrade commands received via MQTT.
    // Retained messages on the commands topic ensure the pod receives the
    // current desired deployment immediately on subscribe, even if the server
    // published before this pod existed.
    let mqtt_active = state.active.clone();
    let mqtt_client = client.clone();
    let mqtt_target = cfg.target.clone();
    let mqtt_cancel = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = mqtt_cancel.cancelled() => break,

                Some(instr) = async {
                    if let Some(ref mut rx) = mqtt_commands { rx.recv().await }
                    else { std::future::pending().await }
                } => {
                    tracing::info!(
                        run_id        = %instr.run_id,
                        deployment_id = %instr.deployment_id,
                        sessions      = instr.sessions,
                        "upgrade command received via MQTT"
                    );
                    let active = mqtt_active.clone();
                    let c      = mqtt_client.clone();
                    let tgt    = mqtt_target.clone();
                    tokio::task::spawn_blocking(move || {
                        deployment::load_and_swap(instr, active, c, tgt);
                    });
                }
            }
        }
    });

    cancel.cancelled().await;
    tracing::info!("inference service stopped");
    Ok(())
}
