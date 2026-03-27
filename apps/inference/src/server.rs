use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use bytes::Bytes;
use edgeflow_common::CancellationToken;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::client::EdgeflowClient;
use crate::pipeline::Pipeline;

#[derive(Default)]
pub struct Metrics {
    pub requests_total:    AtomicU64,
    pub requests_active:   AtomicU64,
    pub requests_rejected: AtomicU64,
    pub inference_errors:  AtomicU64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub run_id: String,
    pub deployment_id: String,
    pub target: String,
    pub loaded_at: String,
}

pub struct ServerState {
    /// Outer RwLock: readers clone the inner Arc in microseconds, releasing
    /// the read lock before inference starts.  Swap (write lock) only needs
    /// to wait for readers to finish that cheap clone — not for in-flight
    /// inference.  Inner Mutex serializes concurrent infer calls on the same
    /// Pipeline instance (required because wasmtime Store needs &mut).
    pub pipeline:  Arc<RwLock<Option<Arc<Mutex<Pipeline>>>>>,
    pub model_info: Arc<RwLock<Option<ModelInfo>>>,
    pub schema:    Arc<RwLock<Option<Vec<u8>>>>,
    pub semaphore: Arc<Semaphore>,
    pub metrics:   Arc<Metrics>,
    pub client:    Arc<EdgeflowClient>,
    pub target:    String,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            pipeline:   self.pipeline.clone(),
            model_info: self.model_info.clone(),
            schema:     self.schema.clone(),
            semaphore:  self.semaphore.clone(),
            metrics:    self.metrics.clone(),
            client:     self.client.clone(),
            target:     self.target.clone(),
        }
    }
}

pub async fn serve(state: ServerState, addr: String, cancel: CancellationToken) -> Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");

    loop {
        let (stream, _peer) = tokio::select! {
            res = listener.accept() => res?,
            _ = cancel.cancelled() => {
                tracing::info!("inference server shutting down");
                break;
            }
        };

        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let svc = service_fn(move |req| handle(req, state.clone()));
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                tracing::warn!("connection error: {e}");
            }
        });
    }

    Ok(())
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
            // Reject immediately if at capacity — don't queue.
            let permit = match state.semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    state.metrics.requests_rejected.fetch_add(1, Ordering::Relaxed);
                    return Ok(Response::builder()
                        .status(StatusCode::TOO_MANY_REQUESTS)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from("{\"error\":\"too many concurrent requests\"}")))
                        .unwrap());
                }
            };

            state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
            state.metrics.requests_active.fetch_add(1, Ordering::Relaxed);

            // Acquire read lock only long enough to clone the inner Arc.
            // The read lock is released before inference starts, so a
            // concurrent swap (write lock) is not blocked by in-flight infer.
            let pipeline_arc = state.pipeline.read().unwrap()
                .as_ref()
                .map(Arc::clone);

            let Some(pipeline_arc) = pipeline_arc else {
                state.metrics.requests_active.fetch_sub(1, Ordering::Relaxed);
                drop(permit);
                return Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from("{\"error\":\"no model loaded yet\"}")))
                    .unwrap());
            };

            let body = req.collect().await?.to_bytes();
            let metrics = state.metrics.clone();
            let result = tokio::task::spawn_blocking(move || {
                let out = pipeline_arc.lock().unwrap().infer(&body);
                drop(permit);  // release as soon as inference completes
                out
            })
            .await
            .unwrap();

            metrics.requests_active.fetch_sub(1, Ordering::Relaxed);

            match result {
                Ok(out) => Ok(Response::new(Full::new(Bytes::from(out)))),
                Err(e) => {
                    metrics.inference_errors.fetch_add(1, Ordering::Relaxed);
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
            let schema = state.schema.clone();
            let client = state.client.clone();
            let target = state.target.clone();
            tokio::task::spawn_blocking(move || {
                load_and_swap(upgrade_req, pipeline, model_info, schema, client, target);
            });

            Ok(Response::builder()
                .status(StatusCode::ACCEPTED)
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from("{\"status\":\"loading\"}")))
                .unwrap())
        }

        (&Method::GET, "/model") => {
            let info = state.model_info.read().unwrap().clone();
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

        (&Method::GET, "/metrics") => {
            let m = &state.metrics;
            let json = serde_json::json!({
                "requests_total":    m.requests_total.load(Ordering::Relaxed),
                "requests_active":   m.requests_active.load(Ordering::Relaxed),
                "requests_rejected": m.requests_rejected.load(Ordering::Relaxed),
                "inference_errors":  m.inference_errors.load(Ordering::Relaxed),
                "concurrency_limit": state.semaphore.available_permits() + m.requests_active.load(Ordering::Relaxed) as usize,
            });
            Ok(Response::builder()
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(serde_json::to_vec(&json).unwrap())))
                .unwrap())
        }

        (&Method::GET, "/schema") => {
            let schema = state.schema.read().unwrap().clone();
            match schema {
                Some(bytes) => Ok(Response::builder()
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from(bytes)))
                    .unwrap()),
                None => Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from("{\"error\":\"no schema available\"}")))
                    .unwrap()),
            }
        }

        (&Method::GET, "/health") => {
            let loaded = state.pipeline.read().unwrap().is_some();
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
    shared_pipeline: Arc<RwLock<Option<Arc<Mutex<Pipeline>>>>>,
    shared_model_info: Arc<RwLock<Option<ModelInfo>>>,
    shared_schema: Arc<RwLock<Option<Vec<u8>>>>,
    client: Arc<EdgeflowClient>,
    target: String,
) {
    let rt = tokio::runtime::Handle::current();

    let result: anyhow::Result<(Pipeline, Option<Vec<u8>>)> = rt.block_on(async {
        tracing::info!(run_id = %req.run_id, "downloading model.onnx");
        let model = client.download_artifact(&req.run_id, "model.onnx").await?;

        let pre_wasm = client.download_artifact(&req.run_id, "preprocess.wasm").await.ok();
        let pre_cfg = client.download_artifact(&req.run_id, "preprocess.json").await.ok();

        let post_wasm = client.download_artifact(&req.run_id, "postprocess.wasm").await.ok();
        let post_cfg = client.download_artifact(&req.run_id, "postprocess.json").await.ok();

        let schema = client.download_artifact(&req.run_id, "schema.json").await.ok();

        Ok((model, pre_wasm, pre_cfg, post_wasm, post_cfg, schema))
    }).and_then(|(model, pre_wasm, pre_cfg, post_wasm, post_cfg, schema)| {
        let pre = pre_wasm.as_deref().map(|w| (w, pre_cfg.as_deref()));
        let post = post_wasm.as_deref().map(|w| (w, post_cfg.as_deref()));
        let backend = crate::backend::build_backend();
        let pipeline = Pipeline::new(backend, &model, pre, post)?;
        Ok((pipeline, schema))
    });

    match result {
        Ok((new_pipeline, schema)) => {
            // Wrap in Arc<Mutex> then write-lock the outer RwLock to swap.
            // Write lock only blocks for readers to finish cloning their Arc,
            // not for in-flight inference calls.
            *shared_pipeline.write().unwrap() = Some(Arc::new(Mutex::new(new_pipeline)));
            *shared_schema.write().unwrap() = schema;
            *shared_model_info.write().unwrap() = Some(ModelInfo {
                run_id: req.run_id,
                deployment_id: req.deployment_id.clone(),
                target,
                loaded_at: chrono::Utc::now().to_rfc3339(),
            });

            tracing::info!(deployment_id = %req.deployment_id, "pipeline swapped successfully");

            let _ = rt.block_on(
                client.confirm_deployment(&req.deployment_id, "deployed", None)
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
