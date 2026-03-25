mod api;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use edgeflow_store::sqlite::SqliteStore;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "edgeflow_server=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_dir = PathBuf::from(std::env::var("EDGEFLOW_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let artifact_root = data_dir.join("artifacts");
    let db_path = data_dir.join("edgeflow.db");

    std::fs::create_dir_all(&artifact_root)?;

    let store = SqliteStore::new(&db_path, artifact_root.clone()).await?;
    let state = AppState { store: Arc::new(store), artifact_root };

    let static_dir = std::env::var("EDGEFLOW_STATIC_DIR").unwrap_or_else(|_| "./static".into());

    let app = Router::new()
        .nest("/api/v1", api::v1_router())
        .nest("/api/2.0/mlflow", api::mlflow_router())
        .nest("/api/2.0/mlflow-artifacts", api::mlflow_artifacts_router())
        .fallback_service(ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::env::var("EDGEFLOW_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
