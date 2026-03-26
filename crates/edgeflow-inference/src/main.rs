mod backend;
mod client;
mod pipeline;
mod server;
mod tensor;
mod wasm;

use std::sync::{Arc, RwLock};
use anyhow::{Context, Result};
use client::EdgeflowClient;
use server::ServerState;
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

    let server_url = std::env::var("EDGEFLOW_SERVER")
        .context("EDGEFLOW_SERVER env var required (e.g. http://edgeflow-server:5000)")?;
    let target = std::env::var("EDGEFLOW_TARGET")
        .context("EDGEFLOW_TARGET env var required (e.g. iris-inference)")?;
    let infer_addr =
        std::env::var("EDGEFLOW_INFER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

    // Derive our reachable address for the server to call back on.
    // k8s injects the pod IP via fieldRef: status.podIP as EDGEFLOW_POD_IP.
    let pod_ip = std::env::var("EDGEFLOW_POD_IP").unwrap_or_else(|_| {
        // Fall back to the bind address (works for local dev where server and
        // inference run on the same host).
        infer_addr
            .split(':')
            .next()
            .unwrap_or("127.0.0.1")
            .replace("0.0.0.0", "127.0.0.1")
            .to_string()
    });
    let port = infer_addr.split(':').last().unwrap_or("8080");
    let self_address = format!("http://{}:{}", pod_ip, port);

    let client = Arc::new(EdgeflowClient::new(&server_url));

    let state = ServerState {
        pipeline: Arc::new(RwLock::new(None)),
        model_info: Arc::new(RwLock::new(None)),
        client: client.clone(),
        target: target.clone(),
    };

    // Start HTTP server in background so we're ready before registering.
    let serve_state = state.clone();
    let serve_addr = infer_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve(serve_state, serve_addr).await {
            tracing::error!("inference server error: {e:#}");
        }
    });

    // Small pause to let the listener bind before we register.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Register with edgeflow-server (retry until it's ready — it may still be
    // starting when we come up).  A successful registration may immediately
    // trigger an /upgrade call back to us if a pending deployment exists.
    tracing::info!(target = %target, address = %self_address, "registering with edgeflow-server");
    loop {
        match client.register_target(&target, &self_address).await {
            Ok(_) => {
                tracing::info!("registered — waiting for /upgrade calls");
                break;
            }
            Err(e) => {
                tracing::warn!("registration failed ({e}), retrying in 3s...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }

    // Keep running until the process is killed.
    std::future::pending::<()>().await;
    Ok(())
}
