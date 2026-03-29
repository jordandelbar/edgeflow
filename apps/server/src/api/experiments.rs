use super::ApiError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use edgeflow_core::ExperimentTag;
use edgeflow_store::Store;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/experiments/create", post(create_experiment))
        .route("/experiments/get", get(get_experiment))
        .route("/experiments/get-by-name", get(get_experiment_by_name))
        .route("/experiments/list", get(list_experiments))
        .route("/experiments/search", post(search_experiments))
        .route("/experiments/delete", post(delete_experiment))
        .route("/experiments/restore", post(restore_experiment))
        .route("/experiments/update", post(update_experiment))
        .route("/experiments/set-experiment-tag", post(set_experiment_tag))
}

#[derive(Deserialize)]
struct CreateExperimentRequest {
    name: String,
    artifact_location: Option<String>,
    tags: Option<Vec<ExperimentTag>>,
}

#[derive(Serialize)]
struct CreateExperimentResponse {
    experiment_id: String,
}

async fn create_experiment(
    State(state): State<AppState>,
    Json(req): Json<CreateExperimentRequest>,
) -> Result<Json<CreateExperimentResponse>, ApiError> {
    let exp = state
        .store
        .create_experiment(
            &req.name,
            req.artifact_location.as_deref(),
            req.tags.unwrap_or_default(),
        )
        .await?;
    Ok(Json(CreateExperimentResponse {
        experiment_id: exp.experiment_id,
    }))
}

#[derive(Deserialize)]
struct GetExperimentQuery {
    experiment_id: String,
}

async fn get_experiment(
    State(state): State<AppState>,
    Query(q): Query<GetExperimentQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let exp = state.store.get_experiment(&q.experiment_id).await?;
    Ok(Json(serde_json::json!({ "experiment": exp })))
}

#[derive(Deserialize)]
struct GetByNameQuery {
    experiment_name: String,
}

async fn get_experiment_by_name(
    State(state): State<AppState>,
    Query(q): Query<GetByNameQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let exp = state
        .store
        .get_experiment_by_name(&q.experiment_name)
        .await?;
    Ok(Json(serde_json::json!({ "experiment": exp })))
}

#[derive(Deserialize)]
struct ListExperimentsQuery {
    view_type: Option<String>,
}

async fn list_experiments(
    State(state): State<AppState>,
    Query(q): Query<ListExperimentsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stage = match q.view_type.as_deref() {
        Some("DELETED_ONLY") => Some(edgeflow_core::LifecycleStage::Deleted),
        _ => None,
    };
    let experiments = state.store.list_experiments(stage).await?;
    Ok(Json(serde_json::json!({ "experiments": experiments })))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SearchExperimentsRequest {
    view_type: Option<String>,
    // max_results, filter, order_by accepted but not yet implemented
    #[serde(default)]
    max_results: Option<i64>,
    #[serde(default)]
    filter: Option<String>,
}

async fn search_experiments(
    State(state): State<AppState>,
    Json(req): Json<SearchExperimentsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stage = match req.view_type.as_deref() {
        Some("DELETED_ONLY") => Some(edgeflow_core::LifecycleStage::Deleted),
        _ => None,
    };
    let experiments = state.store.list_experiments(stage).await?;
    Ok(Json(serde_json::json!({ "experiments": experiments })))
}

#[derive(Deserialize)]
struct DeleteExperimentRequest {
    experiment_id: String,
}

async fn delete_experiment(
    State(state): State<AppState>,
    Json(req): Json<DeleteExperimentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.delete_experiment(&req.experiment_id).await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct RestoreExperimentRequest {
    experiment_id: String,
}

async fn restore_experiment(
    State(state): State<AppState>,
    Json(req): Json<RestoreExperimentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.restore_experiment(&req.experiment_id).await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct UpdateExperimentRequest {
    experiment_id: String,
    new_name: String,
}

async fn update_experiment(
    State(state): State<AppState>,
    Json(req): Json<UpdateExperimentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .update_experiment(&req.experiment_id, &req.new_name)
        .await?;
    Ok(Json(serde_json::json!({})))
}

#[derive(Deserialize)]
struct SetExperimentTagRequest {
    experiment_id: String,
    key: String,
    value: String,
}

async fn set_experiment_tag(
    State(state): State<AppState>,
    Json(req): Json<SetExperimentTagRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .set_experiment_tag(&req.experiment_id, &req.key, &req.value)
        .await?;
    Ok(Json(serde_json::json!({})))
}
