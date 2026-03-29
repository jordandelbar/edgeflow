use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::Result;
use bytes::Bytes;
use edgeflow_common::CancellationToken;
use http_body_util::{BodyExt, Full};
use hyper::{
    body::Incoming, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::client::EdgeflowClient;
use crate::deployment::{self, ActiveDeployment, DeployInstruction};

fn json_error(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    let body = format!(
        "{{\"error\":{}}}",
        serde_json::to_string(msg).unwrap_or_default()
    );
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

fn json_ok(body: impl Into<Bytes>) -> Response<Full<Bytes>> {
    Response::builder()
        .header("content-type", "application/json")
        .body(Full::new(body.into()))
        .unwrap()
}

#[derive(Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub requests_active: AtomicU64,
    pub requests_rejected: AtomicU64,
    pub inference_errors: AtomicU64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub run_id: String,
    pub deployment_id: String,
    pub target: String,
    pub loaded_at: String,
}

pub struct ServerState {
    /// Single RwLock over the entire active deployment: pipeline, model info,
    /// and schema are always updated together, so readers see a consistent
    /// snapshot.  Readers clone the inner Arc in microseconds, releasing the
    /// read lock before any heavy work begins, so a concurrent swap (write
    /// lock) only waits for the cheap clone — not for in-flight inference.
    /// The inner Mutex serialises concurrent infer calls on the same Pipeline
    /// instance (required because wasmtime Store needs &mut).
    pub active: Arc<RwLock<Option<Arc<ActiveDeployment>>>>,
    pub semaphore: Arc<Semaphore>,
    pub metrics: Arc<Metrics>,
    pub client: Arc<EdgeflowClient>,
    pub target: String,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            active: self.active.clone(),
            semaphore: self.semaphore.clone(),
            metrics: self.metrics.clone(),
            client: self.client.clone(),
            target: self.target.clone(),
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
                    state
                        .metrics
                        .requests_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    return Ok(json_error(
                        StatusCode::TOO_MANY_REQUESTS,
                        "too many concurrent requests",
                    ));
                }
            };

            state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
            state
                .metrics
                .requests_active
                .fetch_add(1, Ordering::Relaxed);

            // Acquire read lock only long enough to clone the inner Arc.
            let active = state.active.read().unwrap().as_ref().map(Arc::clone);

            let Some(active) = active else {
                state
                    .metrics
                    .requests_active
                    .fetch_sub(1, Ordering::Relaxed);
                drop(permit);
                return Ok(json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no model loaded yet",
                ));
            };

            let body = req.collect().await?.to_bytes();
            let metrics = state.metrics.clone();
            let result = tokio::task::spawn_blocking(move || {
                let out = active.pipeline.lock().unwrap().infer(&body);
                drop(permit);
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
                    Ok(json_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &e.to_string(),
                    ))
                }
            }
        }

        (&Method::POST, "/upgrade") => {
            let body = req.collect().await?.to_bytes();
            let upgrade_req: UpgradeRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(json_error(
                        StatusCode::BAD_REQUEST,
                        &format!("invalid request: {e}"),
                    ));
                }
            };

            tracing::info!(
                run_id        = %upgrade_req.run_id,
                deployment_id = %upgrade_req.deployment_id,
                "upgrade requested"
            );

            let instr = DeployInstruction {
                run_id: upgrade_req.run_id,
                deployment_id: upgrade_req.deployment_id,
            };
            let active = state.active.clone();
            let client = state.client.clone();
            let target = state.target.clone();
            tokio::task::spawn_blocking(move || {
                deployment::load_and_swap(instr, active, client, target);
            });

            Ok(json_ok(Bytes::from_static(b"{\"status\":\"loading\"}")))
        }

        (&Method::GET, "/model") => {
            let info = state
                .active
                .read()
                .unwrap()
                .as_ref()
                .map(|a| a.model_info.clone());
            match info {
                Some(i) => Ok(json_ok(serde_json::to_vec(&i).unwrap_or_default())),
                None => Ok(json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no model loaded",
                )),
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
            Ok(json_ok(serde_json::to_vec(&json).unwrap()))
        }

        (&Method::GET, "/schema") => {
            let schema = state
                .active
                .read()
                .unwrap()
                .as_ref()
                .and_then(|a| a.schema.clone());
            match schema {
                Some(bytes) => Ok(json_ok(bytes)),
                None => Ok(json_error(StatusCode::NOT_FOUND, "no schema available")),
            }
        }

        (&Method::GET, "/health") => {
            let loaded = state.active.read().unwrap().is_some();
            if loaded {
                Ok(json_ok(Bytes::from_static(b"{\"status\":\"ok\"}")))
            } else {
                Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from_static(b"{\"status\":\"loading\"}")))
                    .unwrap())
            }
        }

        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap()),
    }
}
