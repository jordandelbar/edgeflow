use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use edgeflow_store::Store;
use crate::state::AppState;
use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(create_deployment))
        .route("/deployments/latest", get(get_latest_deployment))
}

#[derive(Deserialize)]
struct CreateDeploymentRequest {
    run_id: String,
    target: String,
}

async fn create_deployment(
    State(state): State<AppState>,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.create_deployment(&req.run_id, &req.target).await?;
    Ok(Json(serde_json::json!({ "deployment": deployment })))
}

#[derive(Deserialize)]
struct LatestDeploymentQuery {
    target: String,
}

async fn get_latest_deployment(
    State(state): State<AppState>,
    Query(q): Query<LatestDeploymentQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.get_latest_deployment(&q.target).await?;
    Ok(Json(serde_json::json!({ "deployment": deployment })))
}
