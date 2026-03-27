use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use edgeflow_core::{RunStatus, RunTag};
use edgeflow_store::Store;
use crate::state::AppState;
use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs/create", post(create_run))
        .route("/runs/get", get(get_run))
        .route("/runs/update", post(update_run))
        .route("/runs/delete", post(delete_run))
        .route("/runs/restore", post(restore_run))
        .route("/runs/search", post(search_runs))
        .route("/runs/log-parameter", post(log_param))
        .route("/runs/set-tag", post(set_tag))
}

#[derive(Deserialize)]
struct CreateRunRequest {
    experiment_id: String,
    run_name: Option<String>,
    start_time: Option<i64>,
    tags: Option<Vec<RunTag>>,
}

async fn create_run(
    State(state): State<AppState>,
    Json(req): Json<CreateRunRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = state.store.create_run(
        &req.experiment_id,
        req.run_name.as_deref(),
        req.start_time,
        req.tags.unwrap_or_default(),
    ).await?;
    Ok(Json(serde_json::json!({ "run": run })))
}

#[derive(Deserialize)]
struct GetRunQuery {
    run_id: String,
}

async fn get_run(
    State(state): State<AppState>,
    Query(q): Query<GetRunQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = state.store.get_run(&q.run_id).await?;
    Ok(Json(serde_json::json!({ "run": run })))
}

#[derive(Deserialize)]
struct UpdateRunRequest {
    run_id: String,
    status: Option<RunStatus>,
    end_time: Option<i64>,
    run_name: Option<String>,
}

async fn update_run(
    State(state): State<AppState>,
    Json(req): Json<UpdateRunRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let info = state.store.update_run(
        &req.run_id,
        req.status.unwrap_or(RunStatus::Running),
        req.end_time,
        req.run_name.as_deref(),
    ).await?;
    Ok(Json(serde_json::json!({ "run_info": info })))
}

#[derive(Deserialize)]
struct RunIdRequest {
    run_id: String,
}

async fn delete_run(
    State(state): State<AppState>,
    Json(req): Json<RunIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.delete_run(&req.run_id).await?;
    Ok(Json(serde_json::json!({})))
}

async fn restore_run(
    State(state): State<AppState>,
    Json(req): Json<RunIdRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.restore_run(&req.run_id).await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct SearchRunsRequest {
    experiment_ids: Vec<String>,
    filter: Option<String>,
    max_results: Option<i64>,
}

async fn search_runs(
    State(state): State<AppState>,
    Json(req): Json<SearchRunsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runs = state.store.search_runs(
        req.experiment_ids,
        req.filter.as_deref(),
        req.max_results.unwrap_or(1000),
    ).await?;
    Ok(Json(serde_json::json!({ "runs": runs })))
}

#[derive(Deserialize)]
struct LogParamRequest {
    run_id: String,
    key: String,
    value: String,
}

async fn log_param(
    State(state): State<AppState>,
    Json(req): Json<LogParamRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.log_param(&req.run_id, &req.key, &req.value).await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct SetTagRequest {
    run_id: String,
    key: String,
    value: String,
}

async fn set_tag(
    State(state): State<AppState>,
    Json(req): Json<SetTagRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.set_tag(&req.run_id, &req.key, &req.value).await?;
    Ok(Json(serde_json::json!({})))
}
