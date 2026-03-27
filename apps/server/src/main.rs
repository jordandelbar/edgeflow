mod api;
mod k8s;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use edgeflow_common::shutdown_signal;
use edgeflow_core::DeploymentState;
use edgeflow_store::sqlite::SqliteStore;
use edgeflow_store::Store;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "edgeflow_server=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cancel = shutdown_signal();

    let data_dir = PathBuf::from(std::env::var("EDGEFLOW_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let artifact_root = data_dir.join("artifacts");
    let db_path = data_dir.join("edgeflow.db");

    std::fs::create_dir_all(&artifact_root)?;

    let store = SqliteStore::new(&db_path, artifact_root.clone()).await?;
    let state = AppState { store: Arc::new(store), artifact_root };

    // Background task: time out deployments stuck in deploying/upgrading.
    let timeout_state = state.clone();
    let timeout_cancel = cancel.clone();
    tokio::spawn(async move {
        let timeout_ms = std::env::var("DEPLOYMENT_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(300)
            * 1000;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                _ = timeout_cancel.cancelled() => { break; }
            }

            match timeout_state.store
                .get_stale_deployments(&["deploying", "upgrading"], timeout_ms)
                .await
            {
                Ok(stale) => {
                    for d in stale {
                        tracing::warn!(
                            deployment_id = %d.deployment_id,
                            target = %d.target,
                            state = %d.state.as_str(),
                            "deployment timed out — marking failed"
                        );
                        let _ = timeout_state.store
                            .update_deployment_state(&d.deployment_id, DeploymentState::Failed)
                            .await;
                    }
                }
                Err(e) => tracing::error!("timeout sweep error: {e}"),
            }
        }
    });

    let static_dir = std::env::var("EDGEFLOW_STATIC_DIR").unwrap_or_else(|_| "./static".into());

    let app = Router::new()
        .nest("/api/v1", api::v1_router())
        .nest("/api/2.0/mlflow", api::mlflow_router())
        .nest("/api/2.0/mlflow-artifacts", api::mlflow_artifacts_router())
        .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html"))))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::env::var("EDGEFLOW_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    Ok(())
}
