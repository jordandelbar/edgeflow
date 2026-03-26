use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::TcpListener;

use crate::client::EdgeflowClient;
use crate::pipeline::Pipeline;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub run_id: String,
    pub deployment_id: String,
    pub target: String,
    pub loaded_at: String,
}

pub struct ServerState {
    pub pipeline: Arc<Mutex<Option<Pipeline>>>,
    pub model_info: Arc<Mutex<Option<ModelInfo>>>,
    pub client: Arc<EdgeflowClient>,
    pub target: String,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            pipeline: self.pipeline.clone(),
            model_info: self.model_info.clone(),
            client: self.client.clone(),
            target: self.target.clone(),
        }
    }
}

pub async fn serve(state: ServerState, addr: String) -> Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("connection from {peer}");
        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let svc = service_fn(move |req| handle(req, state.clone()));
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                tracing::warn!("connection error: {e}");
            }
        });
    }
}

#[derive(Deserialize)]
struct UpgradeRequest {
    run_id: String,
    deployment_id: String,
}

async fn handle(
    req: Request<Incoming>,
    state: ServerState,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/infer") => {
            let pipeline = state.pipeline.clone();
            if pipeline.lock().unwrap().is_none() {
                return Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from("{\"error\":\"no model loaded yet\"}")))
                    .unwrap());
            }

            let body = req.collect().await?.to_bytes();
            let result = tokio::task::spawn_blocking(move || {
                pipeline.lock().unwrap().as_mut().unwrap().infer(&body)
            })
            .await
            .unwrap();

            match result {
                Ok(out) => Ok(Response::new(Full::new(Bytes::from(out)))),
                Err(e) => {
                    tracing::error!("inference error: {e:#}");
                    let msg = format!("{{\"error\":\"{e}\"}}");
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(msg)))
                        .unwrap())
                }
            }
        }

        (&Method::POST, "/upgrade") => {
            let body = req.collect().await?.to_bytes();
            let upgrade_req: UpgradeRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("{{\"error\":\"invalid request: {e}\"}}");
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(msg)))
                        .unwrap());
                }
            };

            tracing::info!(
                run_id = %upgrade_req.run_id,
                deployment_id = %upgrade_req.deployment_id,
                "upgrade requested"
            );

            // Spawn background task — returns 202 immediately.
            let pipeline = state.pipeline.clone();
            let model_info = state.model_info.clone();
            let client = state.client.clone();
            let target = state.target.clone();
            tokio::task::spawn_blocking(move || {
                load_and_swap(upgrade_req, pipeline, model_info, client, target);
            });

            Ok(Response::builder()
                .status(StatusCode::ACCEPTED)
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from("{\"status\":\"loading\"}")))
                .unwrap())
        }

        (&Method::GET, "/model") => {
            let info = state.model_info.lock().unwrap().clone();
            match info {
                Some(i) => {
                    let json = serde_json::to_vec(&i).unwrap_or_default();
                    Ok(Response::builder()
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(json)))
                        .unwrap())
                }
                None => Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from("{\"error\":\"no model loaded\"}")))
                    .unwrap()),
            }
        }

        (&Method::GET, "/health") => {
            let loaded = state.pipeline.lock().unwrap().is_some();
            if loaded {
                Ok(Response::new(Full::new(Bytes::from("{\"status\":\"ok\"}"))))
            } else {
                Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from("{\"status\":\"loading\"}")))
                    .unwrap())
            }
        }

        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap()),
    }
}

/// Blocking function: download artifacts, build new Pipeline, swap atomically.
/// Runs in a `spawn_blocking` thread so wasmtime and ORT are happy.
fn load_and_swap(
    req: UpgradeRequest,
    shared_pipeline: Arc<Mutex<Option<Pipeline>>>,
    shared_model_info: Arc<Mutex<Option<ModelInfo>>>,
    client: Arc<EdgeflowClient>,
    target: String,
) {
    let rt = tokio::runtime::Handle::current();

    let result: anyhow::Result<Pipeline> = rt.block_on(async {
        tracing::info!(run_id = %req.run_id, "downloading model.onnx");
        let model = client.download_artifact(&req.run_id, "model.onnx").await?;

        let pre_wasm = client.download_artifact(&req.run_id, "preprocess.wasm").await.ok();
        let pre_cfg = client.download_artifact(&req.run_id, "preprocess.json").await.ok();

        let post_wasm = client.download_artifact(&req.run_id, "postprocess.wasm").await.ok();
        let post_cfg = client.download_artifact(&req.run_id, "postprocess.json").await.ok();

        Ok((model, pre_wasm, pre_cfg, post_wasm, post_cfg))
    }).and_then(|(model, pre_wasm, pre_cfg, post_wasm, post_cfg)| {
        let pre = pre_wasm.as_deref().map(|w| (w, pre_cfg.as_deref()));
        let post = post_wasm.as_deref().map(|w| (w, post_cfg.as_deref()));
        let backend = crate::backend::build_backend();
        Pipeline::new(backend, &model, pre, post)
    });

    match result {
        Ok(new_pipeline) => {
            // Atomically swap pipeline and model info.
            *shared_pipeline.lock().unwrap() = Some(new_pipeline);
            *shared_model_info.lock().unwrap() = Some(ModelInfo {
                run_id: req.run_id,
                deployment_id: req.deployment_id.clone(),
                target,
                loaded_at: chrono::Utc::now().to_rfc3339(),
            });

            tracing::info!(deployment_id = %req.deployment_id, "pipeline swapped successfully");

            let _ = rt.block_on(
                client.confirm_deployment(&req.deployment_id, "healthy", None)
            );
        }
        Err(e) => {
            tracing::error!(deployment_id = %req.deployment_id, error = %e, "pipeline load failed");
            let _ = rt.block_on(
                client.confirm_deployment(&req.deployment_id, "failed", Some(&e.to_string()))
            );
        }
    }
}
