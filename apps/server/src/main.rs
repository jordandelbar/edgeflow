mod api;
mod mqtt;
mod state;
mod target_client;

use axum::Router;
use edgeflow_common::shutdown_signal;
use edgeflow_config::ServerConfig;
use edgeflow_core::DeploymentState;
use edgeflow_store::sqlite::SqliteStore;
use edgeflow_store::Store;
use edgeflow_telemetry;
use state::AppState;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    edgeflow_telemetry::init("edgeflow-server", "edgeflow_server=info,tower_http=warn")?;

    let cancel = shutdown_signal();

    let cfg = ServerConfig::from_env()?;

    let artifact_root = cfg.data_dir.join("artifacts");
    let db_path = cfg.data_dir.join("edgeflow.db");

    std::fs::create_dir_all(&artifact_root)?;

    let store = SqliteStore::new(&db_path, artifact_root.clone()).await?;
    // AppState is built after MQTT setup so the publisher can be passed in.
    // Declare state as mutable so we can attach the publisher below.
    let mut state = AppState {
        store: Arc::new(store),
        artifact_root,
        http_client: reqwest::Client::new(),
        mqtt_publisher: None,
        prometheus_url: cfg.prometheus_url.clone(),
    };

    // Background task: time out deployments stuck in deploying/upgrading.
    let timeout_state = state.clone();
    let timeout_cancel = cancel.clone();
    let timeout_ms = cfg.deployment_timeout_secs * 1000;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                _ = timeout_cancel.cancelled() => { break; }
            }

            match timeout_state
                .store
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
                        let _ = timeout_state
                            .store
                            .update_deployment_state(&d.deployment_id, DeploymentState::Failed)
                            .await;
                    }
                }
                Err(e) => tracing::error!("timeout sweep error: {e}"),
            }
        }
    });

    // ── MQTT ─────────────────────────────────────────────────────────────────
    // If EDGEFLOW_MQTT_URL is set, connect to that external broker.
    // Otherwise, start the embedded rumqttd broker and connect to it.
    if cfg.mqtt_url.is_none() {
        mqtt::start_embedded_broker(cfg.mqtt_port)?;
        // Give the broker a moment to open its listener before we subscribe.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let mqtt_publisher = mqtt::MqttPublisher::new(cfg.mqtt_url.as_deref(), cfg.mqtt_port);
    state.mqtt_publisher = Some(mqtt_publisher);

    let app = Router::new()
        .nest("/api/v1", api::v1_router())
        .nest("/api/2.0/mlflow", api::mlflow_router())
        .nest("/api/2.0/mlflow-artifacts", api::mlflow_artifacts_router())
        .fallback_service(
            ServeDir::new(&cfg.static_dir)
                .fallback(ServeFile::new(format!("{}/index.html", cfg.static_dir))),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.addr).await?;
    tracing::info!("listening on {}", cfg.addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    Ok(())
}
