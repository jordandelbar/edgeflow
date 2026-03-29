mod backend;
mod client;
mod deployment;
mod inputs;
mod pipeline;
mod server;
mod tensor;
mod wasm;

use anyhow::{Context, Result};
use client::EdgeflowClient;
use edgeflow_common::{backoff::retry_forever, shutdown_signal};
use server::{Metrics, ServerState};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "edgeflow_inference=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cancel = shutdown_signal();

    let server_url = std::env::var("EDGEFLOW_SERVER")
        .context("EDGEFLOW_SERVER env var required (e.g. http://edgeflow-server:5000)")?;
    let target = std::env::var("EDGEFLOW_TARGET")
        .context("EDGEFLOW_TARGET env var required (e.g. iris-inference)")?;
    let infer_addr = std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

    let node_name = std::env::var("EDGEFLOW_NODE_NAME").ok();

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

    let max_concurrent = std::env::var("EDGEFLOW_MAX_CONCURRENT_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8usize);

    let client = Arc::new(EdgeflowClient::new(&server_url));

    let state = ServerState {
        active: Arc::new(RwLock::new(None)),
        semaphore: Arc::new(Semaphore::new(max_concurrent)),
        metrics: Arc::new(Metrics::default()),
        client: client.clone(),
        target: target.clone(),
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

    tracing::info!(target = %target, address = %self_address, node = ?node_name, "registering with edgeflow-server");
    retry_forever("register with edgeflow-server", || {
        let client = client.clone();
        let target = target.clone();
        let self_address = self_address.clone();
        let node = node_name.clone();
        async move {
            client
                .register_target(&target, &self_address, node.as_deref())
                .await
        }
    })
    .await;
    tracing::info!("registered — polling for deployments");

    // Background task: heartbeat every 30 s, poll for pending deployments every 5 s.
    let poll_active = state.active.clone();
    let poll_client = client.clone();
    let poll_target = target.clone();
    let poll_cancel = cancel.clone();
    tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        let mut poll = tokio::time::interval(Duration::from_secs(5));
        heartbeat.tick().await;

        loop {
            tokio::select! {
                _ = poll_cancel.cancelled() => break,

                _ = heartbeat.tick() => {
                    if let Err(e) = poll_client.heartbeat(&poll_target).await {
                        tracing::warn!("heartbeat failed: {e}");
                    }
                }

                _ = poll.tick() => {
                    match poll_client.poll_pending(&poll_target).await {
                        Ok(Some(instr)) => {
                            tracing::info!(
                                run_id        = %instr.run_id,
                                deployment_id = %instr.deployment_id,
                                "picked up pending deployment via poll"
                            );
                            let active = poll_active.clone();
                            let client = poll_client.clone();
                            let tgt    = poll_target.clone();
                            tokio::task::spawn_blocking(move || {
                                deployment::load_and_swap(instr, active, client, tgt);
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
