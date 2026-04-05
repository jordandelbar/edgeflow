use std::sync::{Arc, RwLock};

use anyhow::Result;
use bytes::Bytes;
use edgeflow_common::CancellationToken;
use http_body_util::{BodyExt, Full};
use hyper::{
    body::Incoming, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use opentelemetry::metrics::{Counter, Histogram, ObservableGauge, UpDownCounter};
use opentelemetry::KeyValue;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::client::EdgeflowClient;
use crate::deployment::{self, ActiveDeployment, DeployInstruction};

fn backend_name() -> &'static str {
    if cfg!(feature = "ort-backend") {
        "ort"
    } else if cfg!(feature = "tract-backend") {
        "tract"
    } else {
        "unknown"
    }
}

fn read_memory_rss_bytes() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

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

#[derive(Clone)]
pub struct Metrics {
    requests_total: Counter<u64>,
    requests_active: UpDownCounter<i64>,
    duration: Histogram<f64>,
    target_kv: KeyValue,
    // Kept alive so the observable callback continues to fire.
    _memory_rss: ObservableGauge<u64>,
}

impl Metrics {
    pub fn new(target: &str) -> Self {
        let meter = opentelemetry::global::meter("edgeflow-inference");

        let requests_total = meter
            .u64_counter("inference_requests_total")
            .with_description("Total inference requests by status")
            .build();

        let requests_active = meter
            .i64_up_down_counter("inference_requests_active")
            .with_description("In-flight inference requests")
            .build();

        let duration = meter
            .f64_histogram("inference_duration_seconds")
            .with_description("Inference request duration in seconds")
            .with_unit("s")
            .build();

        let memory_rss = meter
            .u64_observable_gauge("inference_memory_rss_bytes")
            .with_description("Pod RSS memory usage in bytes")
            .with_unit("By")
            .with_callback(|observer| {
                if let Some(rss) = read_memory_rss_bytes() {
                    observer.observe(rss, &[]);
                }
            })
            .build();

        Self {
            requests_total,
            requests_active,
            duration,
            target_kv: KeyValue::new("target", target.to_owned()),
            _memory_rss: memory_rss,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub run_id: String,
    pub deployment_id: String,
    pub target: String,
    pub loaded_at: String,
}

pub struct ServerState {
    pub active: Arc<RwLock<Option<Arc<ActiveDeployment>>>>,
    pub semaphore: Arc<Semaphore>,
    pub metrics: Arc<Metrics>,
    pub client: Arc<EdgeflowClient>,
    pub target: String,
    pub sessions: usize,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            active: self.active.clone(),
            semaphore: self.semaphore.clone(),
            metrics: self.metrics.clone(),
            client: self.client.clone(),
            target: self.target.clone(),
            sessions: self.sessions,
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
    /// Sessions count sent by the server; falls back to the pod's startup default.
    sessions: Option<usize>,
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
                    state.metrics.requests_total.add(
                        1,
                        &[
                            state.metrics.target_kv.clone(),
                            KeyValue::new("status", "rejected"),
                        ],
                    );
                    return Ok(json_error(
                        StatusCode::TOO_MANY_REQUESTS,
                        "too many concurrent requests",
                    ));
                }
            };

            state
                .metrics
                .requests_active
                .add(1, &[state.metrics.target_kv.clone()]);

            // Acquire read lock only long enough to clone the inner Arc.
            let active = state.active.read().unwrap().as_ref().map(Arc::clone);

            let Some(active) = active else {
                state
                    .metrics
                    .requests_active
                    .add(-1, &[state.metrics.target_kv.clone()]);
                drop(permit);
                return Ok(json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no model loaded yet",
                ));
            };

            let body = req.collect().await?.to_bytes();
            let metrics = state.metrics.clone();
            let start = std::time::Instant::now();
            // Create the root span before spawning — move it into the blocking thread
            // so child spans created in pipeline.infer() are correctly nested under it.
            let infer_span = tracing::info_span!(
                "infer",
                target = %state.target,
                backend = backend_name(),
            );
            let result = tokio::task::spawn_blocking(move || {
                let _enter = infer_span.enter();
                let mut pipeline = active.pool.checkout();
                let out = pipeline.infer(&body);
                active.pool.checkin(pipeline);
                drop(permit);
                out
            })
            .await
            .unwrap();

            let duration = start.elapsed().as_secs_f64();
            metrics
                .requests_active
                .add(-1, &[metrics.target_kv.clone()]);
            metrics.duration.record(
                duration,
                &[
                    metrics.target_kv.clone(),
                    KeyValue::new("backend", backend_name()),
                ],
            );

            match result {
                Ok(out) => {
                    metrics.requests_total.add(
                        1,
                        &[metrics.target_kv.clone(), KeyValue::new("status", "ok")],
                    );
                    Ok(Response::new(Full::new(Bytes::from(out))))
                }
                Err(e) => {
                    metrics.requests_total.add(
                        1,
                        &[metrics.target_kv.clone(), KeyValue::new("status", "error")],
                    );
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
                sessions      = ?upgrade_req.sessions,
                fallback      = state.sessions,
                "upgrade requested"
            );

            let instr = DeployInstruction {
                run_id: upgrade_req.run_id,
                deployment_id: upgrade_req.deployment_id,
                sessions: upgrade_req.sessions.unwrap_or(state.sessions),
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
