mod backend;
mod client;
mod pipeline;
mod server;
mod tensor;
mod wasm;

use anyhow::{Context, Result};
use client::EdgeflowClient;
use pipeline::Pipeline;
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

    let client = EdgeflowClient::new(&server_url);

    tracing::info!("waiting for deployment for target={target}...");
    let run_id = loop {
        match client.latest_run_id(&target).await {
            Ok(id) => break id,
            Err(_) => {
                tracing::info!("no deployment yet, retrying in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    };
    tracing::info!("run_id={run_id}");

    tracing::info!("downloading model.onnx");
    let model = client.download_artifact(&run_id, "model.onnx").await?;

    tracing::info!("downloading preprocess.wasm");
    let pre = client.download_artifact(&run_id, "preprocess.wasm").await.ok();
    if pre.is_none() {
        tracing::warn!("preprocess.wasm not found — raw bytes will be passed directly to backend");
    }

    tracing::info!("downloading postprocess.wasm");
    let post = client.download_artifact(&run_id, "postprocess.wasm").await.ok();
    if post.is_none() {
        tracing::warn!("postprocess.wasm not found — raw tensor bytes will be returned directly");
    }

    let backend = build_backend();
    tracing::info!("loading pipeline");
    let pipeline = Pipeline::new(backend, &model, pre.as_deref(), post.as_deref())?;

    server::serve(infer_addr, pipeline).await
}

fn build_backend() -> Box<dyn backend::InferenceBackend> {
    #[cfg(feature = "ort-backend")]
    {
        tracing::info!("using ORT backend");
        return Box::new(backend::ort::OrtBackend::new());
    }
    #[cfg(feature = "tract-backend")]
    {
        tracing::info!("using tract backend");
        return Box::new(backend::tract::TractBackend::new());
    }
    #[cfg(not(any(feature = "ort-backend", feature = "tract-backend")))]
    compile_error!("at least one inference backend feature must be enabled (ort-backend or tract-backend)");
}
