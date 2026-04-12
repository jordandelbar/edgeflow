use super::ApiError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use edgeflow_store::prelude::*;
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        // Registered models
        .route("/registered-models/create", post(create_registered_model))
        .route("/registered-models/get", get(get_registered_model))
        .route("/registered-models/update", patch(update_registered_model))
        .route("/registered-models/delete", delete(delete_registered_model))
        .route("/registered-models/list", get(list_registered_models))
        .route("/registered-models/search", post(search_registered_models))
        .route(
            "/registered-models/get-latest-versions",
            get(get_latest_versions),
        )
        // Model versions
        .route("/model-versions/create", post(create_model_version))
        .route("/model-versions/get", get(get_model_version))
        .route("/model-versions/update", patch(update_model_version))
        .route("/model-versions/delete", delete(delete_model_version))
        .route("/model-versions/search", post(search_model_versions))
        .route("/model-versions/transition-stage", post(transition_stage))
}

// ── Registered models ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateRegisteredModelRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn create_registered_model(
    State(state): State<AppState>,
    Json(req): Json<CreateRegisteredModelRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let model = state
        .store
        .create_registered_model(&req.name, req.description.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "registered_model": model })))
}

#[derive(Deserialize)]
struct ModelNameQuery {
    name: String,
}

async fn get_registered_model(
    State(state): State<AppState>,
    Query(q): Query<ModelNameQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let model = state.store.get_registered_model(&q.name).await?;
    Ok(Json(serde_json::json!({ "registered_model": model })))
}

#[derive(Deserialize)]
struct UpdateRegisteredModelRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn update_registered_model(
    State(state): State<AppState>,
    Json(req): Json<UpdateRegisteredModelRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let model = state
        .store
        .update_registered_model(&req.name, req.description.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "registered_model": model })))
}

#[derive(Deserialize)]
struct DeleteRegisteredModelRequest {
    name: String,
}

async fn delete_registered_model(
    State(state): State<AppState>,
    Json(req): Json<DeleteRegisteredModelRequest>,
) -> Result<StatusCode, ApiError> {
    state.store.delete_registered_model(&req.name).await?;
    Ok(StatusCode::OK)
}

async fn list_registered_models(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let models = state.store.list_registered_models().await?;
    Ok(Json(serde_json::json!({ "registered_models": models })))
}

#[derive(Deserialize)]
struct SearchRegisteredModelsRequest {
    #[serde(default)]
    filter: Option<String>,
    #[serde(default = "default_max")]
    max_results: i64,
}

fn default_max() -> i64 {
    200
}

async fn search_registered_models(
    State(state): State<AppState>,
    Json(_req): Json<SearchRegisteredModelsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Simple implementation: return all (filter not parsed yet)
    let models = state.store.list_registered_models().await?;
    Ok(Json(serde_json::json!({ "registered_models": models })))
}

#[derive(Deserialize)]
struct LatestVersionsQuery {
    name: String,
    /// Comma-separated stages, e.g. "None,Staging,Production"
    #[serde(default)]
    stages: Option<String>,
}

async fn get_latest_versions(
    State(state): State<AppState>,
    Query(q): Query<LatestVersionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stages: Vec<&str> = q
        .stages
        .as_deref()
        .map(|s| s.split(',').collect())
        .unwrap_or_default();
    let versions = state
        .store
        .get_latest_model_versions(&q.name, &stages)
        .await?;
    Ok(Json(serde_json::json!({ "model_versions": versions })))
}

// ── Model versions ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateModelVersionRequest {
    name: String,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

async fn create_model_version(
    State(state): State<AppState>,
    Json(req): Json<CreateModelVersionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let version = state
        .store
        .create_model_version(
            &req.name,
            req.run_id.as_deref(),
            req.source.as_deref(),
            req.description.as_deref(),
        )
        .await?;
    Ok(Json(serde_json::json!({ "model_version": version })))
}

#[derive(Deserialize)]
struct ModelVersionQuery {
    name: String,
    version: String,
}

async fn get_model_version(
    State(state): State<AppState>,
    Query(q): Query<ModelVersionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let v: i64 = q
        .version
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid version: {}", q.version))?;
    let version = state.store.get_model_version(&q.name, v).await?;
    Ok(Json(serde_json::json!({ "model_version": version })))
}

#[derive(Deserialize)]
struct UpdateModelVersionRequest {
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
}

async fn update_model_version(
    State(state): State<AppState>,
    Json(req): Json<UpdateModelVersionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let v: i64 = req
        .version
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid version: {}", req.version))?;
    let version = state
        .store
        .update_model_version(&req.name, v, req.description.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "model_version": version })))
}

#[derive(Deserialize)]
struct DeleteModelVersionRequest {
    name: String,
    version: String,
}

async fn delete_model_version(
    State(state): State<AppState>,
    Json(req): Json<DeleteModelVersionRequest>,
) -> Result<StatusCode, ApiError> {
    let v: i64 = req
        .version
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid version: {}", req.version))?;
    state.store.delete_model_version(&req.name, v).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct SearchModelVersionsRequest {
    #[serde(default)]
    filter: Option<String>,
}

async fn search_model_versions(
    State(state): State<AppState>,
    Json(req): Json<SearchModelVersionsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let versions = state
        .store
        .search_model_versions(req.filter.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "model_versions": versions })))
}

#[derive(Deserialize)]
struct TransitionStageRequest {
    name: String,
    version: String,
    stage: String,
    #[serde(default)]
    archive_existing_versions: bool,
}

async fn transition_stage(
    State(state): State<AppState>,
    Json(req): Json<TransitionStageRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let v: i64 = req
        .version
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid version: {}", req.version))?;
    if req.archive_existing_versions {
        // Archive all other versions currently in that stage
        let existing = state
            .store
            .get_latest_model_versions(&req.name, &[&req.stage])
            .await?;
        for mv in existing {
            let mv_v: i64 = mv.version.parse().unwrap_or(0);
            if mv_v != v {
                state
                    .store
                    .transition_model_version_stage(&req.name, mv_v, "Archived")
                    .await?;
            }
        }
    }
    let version = state
        .store
        .transition_model_version_stage(&req.name, v, &req.stage)
        .await?;
    Ok(Json(serde_json::json!({ "model_version": version })))
}
