use super::ApiError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use edgeflow_core::{Metric, Param, RunTag};
use edgeflow_store::prelude::*;
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs/log-metric", post(log_metric))
        .route("/runs/log-batch", post(log_batch))
        .route("/metrics/get-history", get(get_metric_history))
}

#[derive(Deserialize)]
struct LogMetricRequest {
    run_id: String,
    key: String,
    value: f64,
    timestamp: i64,
    step: Option<i64>,
}

async fn log_metric(
    State(state): State<AppState>,
    Json(req): Json<LogMetricRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .log_metric(
            &req.run_id,
            Metric {
                key: req.key,
                value: req.value,
                timestamp: req.timestamp,
                step: req.step.unwrap_or(0),
            },
        )
        .await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct LogBatchRequest {
    run_id: String,
    metrics: Option<Vec<Metric>>,
    params: Option<Vec<Param>>,
    tags: Option<Vec<RunTag>>,
}

async fn log_batch(
    State(state): State<AppState>,
    Json(req): Json<LogBatchRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .log_batch(
            &req.run_id,
            req.metrics.unwrap_or_default(),
            req.params.unwrap_or_default(),
            req.tags.unwrap_or_default(),
        )
        .await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct MetricHistoryQuery {
    run_id: String,
    metric_key: String,
}

async fn get_metric_history(
    State(state): State<AppState>,
    Query(q): Query<MetricHistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let metrics = state
        .store
        .get_metric_history(&q.run_id, &q.metric_key)
        .await?;
    Ok(Json(serde_json::json!({ "metrics": metrics })))
}
