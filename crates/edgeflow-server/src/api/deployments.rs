use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use edgeflow_core::DeploymentState;
use edgeflow_store::Store;
use crate::state::AppState;
use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(create_deployment))
        .route("/deployments", get(list_deployments))
        .route("/deployments/latest", get(get_latest_deployment))
        .route("/deployments/:id", get(get_deployment))
        .route("/deployments/:id/confirm", post(confirm_deployment))
        .route("/targets/register", post(register_target))
}

// ── POST /deployments ────────────────────────────────────────────────────────

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

    // Check if we already have a registered address for this target.
    if let Some(target_rec) = state.store.get_target(&req.target).await? {
        // Upgrade path: pod is alive, tell it to load the new run.
        let body = serde_json::json!({
            "run_id": deployment.run_id,
            "deployment_id": deployment.deployment_id,
        });
        let upgrade_url = format!("{}/upgrade", target_rec.address);
        match reqwest::Client::new().post(&upgrade_url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 202 => {
                state.store
                    .update_deployment_state(&deployment.deployment_id, DeploymentState::Upgrading)
                    .await?;
            }
            Ok(resp) => {
                tracing::warn!(
                    deployment_id = %deployment.deployment_id,
                    status = %resp.status(),
                    "upgrade call to pod returned non-success"
                );
            }
            Err(e) => {
                tracing::warn!(
                    deployment_id = %deployment.deployment_id,
                    error = %e,
                    "failed to reach inference pod for upgrade"
                );
            }
        }
    } else {
        // First deploy: pod doesn't exist yet — create it via k8s.
        crate::k8s::create_inference_pod(&req.target).await;
    }

    let deployment = state.store.get_deployment(&deployment.deployment_id).await?;
    Ok(Json(serde_json::json!({ "deployment": deployment })))
}

// ── GET /deployments?target=<t> ──────────────────────────────────────────────

#[derive(Deserialize)]
struct DeploymentQuery {
    target: Option<String>,
}

async fn list_deployments(
    State(state): State<AppState>,
    Query(q): Query<DeploymentQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployments = state.store.list_deployments(q.target.as_deref()).await?;
    Ok(Json(serde_json::json!({ "deployments": deployments })))
}

// ── GET /deployments/latest ──────────────────────────────────────────────────

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

// ── GET /deployments/:id ─────────────────────────────────────────────────────

async fn get_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.get_deployment(&id).await?;
    Ok(Json(serde_json::json!({ "deployment": deployment })))
}

// ── POST /deployments/:id/confirm ────────────────────────────────────────────

#[derive(Deserialize)]
struct ConfirmRequest {
    status: String,  // "healthy" | "failed"
    reason: Option<String>,
}

async fn confirm_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.get_deployment(&id).await?;

    match req.status.as_str() {
        "healthy" => {
            state.store
                .update_deployment_state(&id, DeploymentState::Healthy)
                .await?;
            // If this was an upgrade, supersede the previous healthy deployment.
            if deployment.state == DeploymentState::Upgrading {
                state.store
                    .supersede_previous_deployments(&deployment.target, &id)
                    .await?;
            }
            tracing::info!(
                deployment_id = %id,
                target = %deployment.target,
                "deployment confirmed healthy"
            );
        }
        "failed" => {
            state.store
                .update_deployment_state(&id, DeploymentState::Failed)
                .await?;
            tracing::warn!(
                deployment_id = %id,
                target = %deployment.target,
                reason = ?req.reason,
                "deployment failed"
            );
        }
        other => {
            return Err(anyhow::anyhow!("unknown status: {other}").into());
        }
    }

    let deployment = state.store.get_deployment(&id).await?;
    Ok(Json(serde_json::json!({ "deployment": deployment })))
}

// ── POST /targets/register ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct RegisterTargetRequest {
    target: String,
    address: String,
    pod_name: Option<String>,
}

async fn register_target(
    State(state): State<AppState>,
    Json(req): Json<RegisterTargetRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let target = state.store
        .register_target(&req.target, &req.address, req.pod_name.as_deref())
        .await?;

    // Check for a pending deployment for this target — trigger the load.
    if let Some(deployment) = state.store
        .get_pending_deployment_for_target(&req.target)
        .await?
    {
        let body = serde_json::json!({
            "run_id": deployment.run_id,
            "deployment_id": deployment.deployment_id,
        });
        let upgrade_url = format!("{}/upgrade", req.address);
        match reqwest::Client::new().post(&upgrade_url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 202 => {
                state.store
                    .update_deployment_state(&deployment.deployment_id, DeploymentState::Deploying)
                    .await?;
                tracing::info!(
                    deployment_id = %deployment.deployment_id,
                    target = %req.target,
                    "triggered first deploy on newly registered pod"
                );
            }
            Ok(resp) => {
                tracing::warn!(
                    status = %resp.status(),
                    "upgrade call to pod after registration returned non-success"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to reach newly registered pod");
            }
        }
    }

    Ok(Json(serde_json::json!({ "target": target })))
}
