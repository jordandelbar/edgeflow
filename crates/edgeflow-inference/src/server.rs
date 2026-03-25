use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::pipeline::Pipeline;

type SharedPipeline = Arc<Mutex<Pipeline>>;

pub async fn serve(addr: String, pipeline: Pipeline) -> Result<()> {
    let pipeline: SharedPipeline = Arc::new(Mutex::new(pipeline));
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("connection from {peer}");
        let io = TokioIo::new(stream);
        let pipeline = pipeline.clone();

        tokio::spawn(async move {
            let svc = service_fn(move |req| handle(req, pipeline.clone()));
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                tracing::warn!("connection error: {e}");
            }
        });
    }
}

async fn handle(
    req: Request<Incoming>,
    pipeline: SharedPipeline,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/infer") => {
            let body = req.collect().await?.to_bytes();

            // Run inference on a blocking thread — wasmtime Store is not async-safe.
            let result = tokio::task::spawn_blocking(move || {
                pipeline.lock().unwrap().infer(&body)
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
        (&Method::GET, "/health") => {
            Ok(Response::new(Full::new(Bytes::from("{\"status\":\"ok\"}"))))
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .unwrap()),
    }
}
