mod artifacts;
mod experiments;
mod metrics;
mod runs;

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use axum::Router;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(experiments::router())
        .merge(runs::router())
        .merge(metrics::router())
        .merge(artifacts::router())
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
