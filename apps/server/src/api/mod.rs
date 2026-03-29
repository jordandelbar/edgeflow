mod artifacts;
mod deployments;
mod experiments;
mod metrics;
mod model_registry;
mod runs;

use crate::state::AppState;
use axum::Router;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

/// MLflow-compatible API surface (/api/2.0/mlflow/*)
pub fn mlflow_router() -> Router<AppState> {
    Router::new()
        .merge(experiments::router())
        .merge(runs::router())
        .merge(metrics::router())
        .merge(artifacts::router())
        .merge(model_registry::router())
}

/// Artifact proxy upload API (/api/2.0/mlflow-artifacts/*)
pub fn mlflow_artifacts_router() -> Router<AppState> {
    artifacts::mlflow_artifacts_router()
}

/// Native edgeflow API (/api/v1/*)
pub fn v1_router() -> Router<AppState> {
    Router::new().merge(deployments::router())
}

/// Unified error type that maps to MLflow-style error JSON.
pub struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let msg = self.0.to_string();
        // Map common error messages to MLflow error codes + HTTP status.
        // The MLflow client stops retrying on 404 RESOURCE_DOES_NOT_EXIST
        // and treats 500 INTERNAL_ERROR as a transient failure worth retrying.
        let (status, error_code) = if msg.contains("not found") || msg.contains("no rows") {
            (StatusCode::NOT_FOUND, "RESOURCE_DOES_NOT_EXIST")
        } else if msg.contains("already exists") || msg.contains("UNIQUE constraint") {
            (StatusCode::BAD_REQUEST, "RESOURCE_ALREADY_EXISTS")
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
        };
        let body = serde_json::json!({ "error_code": error_code, "message": msg });
        (status, Json(body)).into_response()
    }
}
