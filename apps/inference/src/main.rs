mod backend;
mod client;
mod inputs;
mod pipeline;
mod server;
mod tensor;
mod wasm;

use std::sync::{Arc, RwLock};
use anyhow::{Context, Result};
use client::EdgeflowClient;
use edgeflow_common::{backoff::retry_forever, shutdown_signal};
use server::{Metrics, ServerState};
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
    let infer_addr =
        std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

    // Derive our reachable address for the server to call back on.
    // k8s injects the pod IP via fieldRef: status.podIP as EDGEFLOW_POD_IP.
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
        pipeline:   Arc::new(RwLock::new(None)),
        model_info: Arc::new(RwLock::new(None)),
        schema:     Arc::new(RwLock::new(None)),
        semaphore:  Arc::new(Semaphore::new(max_concurrent)),
        metrics:    Arc::new(Metrics::default()),
        client:     client.clone(),
        target:     target.clone(),
    };

    // Start HTTP server in background so we're ready before registering.
    let serve_state = state.clone();
    let serve_addr = infer_addr.clone();
    let serve_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(serve_state, serve_addr, serve_cancel).await {
            tracing::error!("inference server error: {e:#}");
        }
    });

    // Small pause to let the listener bind before we register.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Register with edgeflow-server (retry with exponential backoff until it's
    // ready — it may still be starting when we come up).
    tracing::info!(target = %target, address = %self_address, "registering with edgeflow-server");
    retry_forever("register with edgeflow-server", || {
        let client = client.clone();
        let target = target.clone();
        let self_address = self_address.clone();
        async move { client.register_target(&target, &self_address).await }
    })
    .await;
    tracing::info!("registered — waiting for /upgrade calls");

    cancel.cancelled().await;
    tracing::info!("inference service stopped");
    Ok(())
}
