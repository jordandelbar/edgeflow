use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct TargetStats {
    pub rps: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub cpu_ratio: Option<f64>,
}

pub fn router() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new().route("/targets/{target}/stats", get(get_target_stats))
}

async fn get_target_stats(
    Path(target): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TargetStats>, StatusCode> {
    let prom_url = state
        .prometheus_url
        .as_deref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let rps = query_scalar(
        &state.http_client,
        prom_url,
        &format!("sum(rate(edgeflow_inference_requests_total{{target=\"{target}\"}}[2m]))"),
    )
    .await;

    let p50_ms = query_scalar(
        &state.http_client,
        prom_url,
        &format!(
            "histogram_quantile(0.50, sum by (le) \
             (rate(edgeflow_inference_duration_seconds_bucket{{target=\"{target}\"}}[2m]))) * 1000"
        ),
    )
    .await;

    let p95_ms = query_scalar(
        &state.http_client,
        prom_url,
        &format!(
            "histogram_quantile(0.95, sum by (le) \
             (rate(edgeflow_inference_duration_seconds_bucket{{target=\"{target}\"}}[2m]))) * 1000"
        ),
    )
    .await;

    let p99_ms = query_scalar(
        &state.http_client,
        prom_url,
        &format!(
            "histogram_quantile(0.99, sum by (le) \
             (rate(edgeflow_inference_duration_seconds_bucket{{target=\"{target}\"}}[2m]))) * 1000"
        ),
    )
    .await;

    let memory_bytes = query_scalar(
        &state.http_client,
        prom_url,
        &format!("edgeflow_inference_memory_rss_bytes{{target=\"{target}\"}}"),
    )
    .await
    .map(|v| v as u64);

    let cpu_ratio = query_scalar(
        &state.http_client,
        prom_url,
        &format!("edgeflow_inference_cpu_usage_ratio{{target=\"{target}\"}}"),
    )
    .await;

    Ok(Json(TargetStats {
        rps,
        p50_ms,
        p95_ms,
        p99_ms,
        memory_bytes,
        cpu_ratio,
    }))
}

/// Query Prometheus for a single scalar value. Returns `None` on any error or
/// if the query returns no results.
async fn query_scalar(client: &reqwest::Client, base_url: &str, expr: &str) -> Option<f64> {
    let mut url = reqwest::Url::parse(&format!("{base_url}/api/v1/query")).ok()?;
    url.query_pairs_mut().append_pair("query", expr);
    let body: serde_json::Value = client.get(url).send().await.ok()?.json().await.ok()?;

    let value_str = body["data"]["result"].as_array()?.first()?["value"][1].as_str()?;

    let v: f64 = value_str.parse().ok()?;
    // Prometheus returns NaN for histogram_quantile with no data
    if v.is_nan() || v.is_infinite() {
        None
    } else {
        Some(v)
    }
}
