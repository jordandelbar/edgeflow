use axum::{
    body::Body,
    extract::{Query, State},
    http::StatusCode,
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use edgeflow_store::Store;
use crate::state::AppState;
use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/artifacts/list", get(list_artifacts))
        .route("/artifacts/get-artifact", get(get_artifact))
}

#[derive(Deserialize)]
struct ListArtifactsQuery {
    run_id: String,
    path: Option<String>,
}

async fn list_artifacts(
    State(state): State<AppState>,
    Query(q): Query<ListArtifactsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let files = state.store.list_artifacts(&q.run_id, q.path.as_deref()).await?;
    let root = state.store.artifact_root(&q.run_id).await?;
    Ok(Json(serde_json::json!({
        "root_uri": root.display().to_string(),
        "files": files
    })))
}

#[derive(Deserialize)]
struct GetArtifactQuery {
    run_id: String,
    path: String,
}

async fn get_artifact(
    State(state): State<AppState>,
    Query(q): Query<GetArtifactQuery>,
) -> Result<Response<Body>, ApiError> {
    let root = state.store.artifact_root(&q.run_id).await?;
    let file_path = root.join(&q.path);

    // Prevent path traversal
    let canonical = file_path.canonicalize().map_err(|_| ApiError::from(anyhow::anyhow!("not found")))?;
    let canonical_root = root.canonicalize().map_err(|_| ApiError::from(anyhow::anyhow!("not found")))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::from(anyhow::anyhow!("not found")));
    }

    let file = File::open(&canonical).await.map_err(|_| ApiError::from(anyhow::anyhow!("not found")))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .body(body)
        .unwrap())
}
