// Core inference modules live in lib.rs, so tests and benches can import them.
mod client;
mod deployment;
mod mqtt;
mod server;

use anyhow::{Context, Result};
use client::EdgeflowClient;
use edgeflow_common::{backoff::retry_forever, shutdown_signal};
use server::{Metrics, ServerState};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Semaphore;
#[tokio::main]
async fn main() -> Result<()> {
    edgeflow_common::logging::init_logging("edgeflow_inference=info");

    let cancel = shutdown_signal();

    let server_url = std::env::var("EDGEFLOW_SERVER")
        .context("EDGEFLOW_SERVER env var required (e.g. http://edgeflow-server:5000)")?;
    let target = std::env::var("EDGEFLOW_TARGET")
        .context("EDGEFLOW_TARGET env var required (e.g. iris-inference)")?;
    let infer_addr = std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

    let node_name = std::env::var("EDGEFLOW_NODE_NAME").ok();
    // Pod identity: use the k8s pod name if available, fall back to the target name.
    let pod_id = std::env::var("EDGEFLOW_POD_NAME").unwrap_or_else(|_| target.clone());

    let pod_ip = std::env::var("EDGEFLOW_POD_IP").unwrap_or_else(|_| {
        infer_addr
            .split(':')
            .next()
            .unwrap_or("127.0.0.1")
            .replace("0.0.0.0", "127.0.0.1")
            .to_string()
    });
    let port = infer_addr.split(':').last().unwrap_or("8080");
    let self_address = format!("http://{}:{}", pod_ip, port);

    let sessions = std::env::var("EDGEFLOW_SESSIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1usize);
    let max_concurrent = std::env::var("EDGEFLOW_MAX_CONCURRENT_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(sessions);

    let client = Arc::new(EdgeflowClient::new(&server_url));

    let state = ServerState {
        active: Arc::new(RwLock::new(None)),
        semaphore: Arc::new(Semaphore::new(max_concurrent)),
        metrics: Arc::new(Metrics::default()),
        client: client.clone(),
        target: target.clone(),
        sessions,
    };

    // Start the HTTP server in the background so we're ready before registering.
    let serve_state = state.clone();
    let serve_addr = infer_addr.clone();
    let serve_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(serve_state, serve_addr, serve_cancel).await {
            tracing::error!("inference server error: {e:#}");
        }
    });

    // Small pause to let the listener bind before we register.
    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!(target = %target, pod_id = %pod_id, address = %self_address, node = ?node_name, "registering with edgeflow-server");
    retry_forever("register with edgeflow-server", || {
        let client = client.clone();
        let target = target.clone();
        let pod_id = pod_id.clone();
        let self_address = self_address.clone();
        let node = node_name.clone();
        async move {
            client
                .register_pod(&pod_id, &target, &self_address, node.as_deref())
                .await
        }
    })
    .await;
    tracing::info!("registered — polling for deployments");

    // If EDGEFLOW_MQTT_URL is set, connect for upgrade commands.
    let mut mqtt_commands: Option<tokio::sync::mpsc::Receiver<deployment::DeployInstruction>> =
        match std::env::var("EDGEFLOW_MQTT_URL").ok() {
            Some(url) => match mqtt::MqttPodClient::new(&url, &target, &pod_id) {
                Ok((_client, rx)) => Some(rx),
                Err(e) => {
                    tracing::warn!("mqtt init failed: {e}");
                    None
                }
            },
            None => None,
        };

    // Background task: poll for pending deployments every 5 s and process
    // upgrade commands received via MQTT.
    let poll_active = state.active.clone();
    let poll_client = client.clone();
    let poll_target = target.clone();
    let poll_cancel = cancel.clone();
    let poll_sessions = state.sessions;
    tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = poll_cancel.cancelled() => break,

                // Upgrade command received via MQTT — act immediately.
                Some(instr) = async {
                    if let Some(ref mut rx) = mqtt_commands { rx.recv().await }
                    else { std::future::pending().await }
                } => {
                    tracing::info!(
                        run_id        = %instr.run_id,
                        deployment_id = %instr.deployment_id,
                        sessions      = instr.sessions,
                        "picked up upgrade command via MQTT"
                    );
                    let active = poll_active.clone();
                    let c      = poll_client.clone();
                    let tgt    = poll_target.clone();
                    tokio::task::spawn_blocking(move || {
                        deployment::load_and_swap(instr, active, c, tgt);
                    });
                }

                _ = poll.tick() => {
                    match poll_client.poll_pending(&poll_target, poll_sessions).await {
                        Ok(Some(instr)) => {
                            tracing::info!(
                                run_id        = %instr.run_id,
                                deployment_id = %instr.deployment_id,
                                sessions      = instr.sessions,
                                "picked up pending deployment via poll"
                            );
                            let active = poll_active.clone();
                            let c      = poll_client.clone();
                            let tgt    = poll_target.clone();
                            tokio::task::spawn_blocking(move || {
                                deployment::load_and_swap(instr, active, c, tgt);
                            });
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("deployment poll failed: {e}"),
                    }
                }
            }
        }
    });

    cancel.cancelled().await;
    tracing::info!("inference service stopped");
    Ok(())
}
