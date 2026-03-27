use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use edgeflow_core::{DeploymentState, ResourceSettings};
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
        .route("/targets", get(list_targets))
        .route("/targets/register", post(register_target))
        .route("/targets/:target/model", get(target_model_status))
        .route("/targets/:target/schema", get(target_schema))
        .route("/targets/:target/health", get(target_health))
        .route("/targets/:target/infer/playground", post(infer_playground))
        .route("/targets/:target", delete(teardown_target))
        .route("/nodes", get(list_nodes))
}

// ── Tensor helpers (mirrors edgeflow-inference tensor format) ─────────────────

fn tensor_decode(bytes: &[u8]) -> anyhow::Result<(Vec<usize>, Vec<f32>)> {
    anyhow::ensure!(!bytes.is_empty(), "empty tensor buffer");
    let mut pos = 0;
    let ndim = bytes[pos] as usize;
    pos += 1;
    anyhow::ensure!(bytes.len() >= pos + ndim * 4 + 1, "buffer too short for shape");
    let mut shape = Vec::with_capacity(ndim);
    for _ in 0..ndim {
        let dim = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        shape.push(dim);
        pos += 4;
    }
    let dtype = bytes[pos];
    pos += 1;
    anyhow::ensure!(dtype == 1, "unsupported dtype {dtype}, only f32 supported");
    let data = bytes[pos..].chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    Ok((shape, data))
}

// ── GET /targets/:target/model ────────────────────────────────────────────────

async fn target_model_status(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = state.store.get_target(&target).await?
        .ok_or_else(|| anyhow::anyhow!("target '{target}' not registered"))?;

    let url = format!("{}/model", rec.address);
    let resp = reqwest::Client::new().get(&url).send().await
        .map_err(|e| anyhow::anyhow!("failed to reach inference pod: {e}"))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("no model loaded on target '{target}'").into());
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("failed to parse model info: {e}"))?;

    Ok(Json(json))
}

// ── GET /targets/:target/health ───────────────────────────────────────────────

async fn target_health(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = state.store.get_target(&target).await?
        .ok_or_else(|| anyhow::anyhow!("target '{target}' not registered"))?;

    let url = format!("{}/health", rec.address);
    let resp = reqwest::Client::new().get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send().await
        .map_err(|_| anyhow::anyhow!("pod unreachable"))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("failed to parse health response: {e}"))?;

    Ok(Json(json))
}

// ── GET /targets/:target/schema ───────────────────────────────────────────────

async fn target_schema(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = state.store.get_target(&target).await?
        .ok_or_else(|| anyhow::anyhow!("target '{target}' not registered"))?;

    let url = format!("{}/schema", rec.address);
    let resp = reqwest::Client::new().get(&url).send().await
        .map_err(|e| anyhow::anyhow!("failed to reach inference pod: {e}"))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("no schema available on target '{target}'").into());
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("failed to parse schema: {e}"))?;

    Ok(Json(json))
}

// ── POST /targets/:target/infer/playground ────────────────────────────────────

#[derive(Deserialize)]
struct PlaygroundRequest {
    data: Vec<f32>,
}

async fn infer_playground(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Json(req): Json<PlaygroundRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = state.store.get_target(&target).await?
        .ok_or_else(|| anyhow::anyhow!("target '{target}' not registered"))?;

    // Send raw packed floats — same format as the Python client (struct.pack('<Nf', ...)).
    // The preprocess WASM (FloatBytesToTensor) expects this, not a tensor-encoded header.
    let body: Vec<u8> = req.data.iter().flat_map(|&v| v.to_le_bytes()).collect();
    let url = format!("{}/infer", rec.address);

    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/octet-stream")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to reach inference pod: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let msg = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("inference pod returned {status}: {msg}").into());
    }

    let bytes = resp.bytes().await
        .map_err(|e| anyhow::anyhow!("failed to read response: {e}"))?;

    // The postprocess WASM can return anything. Try JSON first (ClassifierOutput,
    // custom transforms), then fall back to tensor decode (no postprocess).
    let result = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        json
    } else if let Ok((shape, data)) = tensor_decode(&bytes) {
        serde_json::json!({ "shape": shape, "data": data })
    } else {
        return Err(anyhow::anyhow!("unrecognised response format from inference pod").into());
    };

    Ok(Json(result))
}

// ── POST /deployments ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateDeploymentRequest {
    run_id: String,
    target: String,
    node: Option<String>,
    #[serde(default)]
    resources: ResourceSettings,
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
        // First deploy: pod doesn't exist yet — store node + resources then create it via k8s.
        state.store.store_target_resources(&req.target, req.node.as_deref(), &req.resources).await?;
        crate::k8s::create_inference_pod(&req.target, req.node.as_deref(), &req.resources).await;
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
    status: String,  // "deployed" | "failed"
    reason: Option<String>,
}

async fn confirm_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.get_deployment(&id).await?;

    match req.status.as_str() {
        "deployed" => {
            state.store
                .update_deployment_state(&id, DeploymentState::Deployed)
                .await?;
            // If this was an upgrade, supersede the previous deployed deployment.
            if deployment.state == DeploymentState::Upgrading {
                state.store
                    .supersede_previous_deployments(&deployment.target, &id)
                    .await?;
            }
            tracing::info!(
                deployment_id = %id,
                target = %deployment.target,
                "deployment confirmed deployed"
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

// ── DELETE /targets/:target ───────────────────────────────────────────────────

async fn teardown_target(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Supersede active deployments + remove target record.
    state.store.delete_target(&target).await?;

    // Best-effort k8s cleanup — logs a warning if cluster is unreachable.
    crate::k8s::delete_inference_pod(&target).await;

    tracing::info!(target = %target, "target torn down");
    Ok(StatusCode::NO_CONTENT)
}

// ── GET /targets ──────────────────────────────────────────────────────────────

async fn list_targets(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let targets = state.store.list_targets().await?;
    Ok(Json(serde_json::json!({ "targets": targets })))
}

// ── GET /nodes ────────────────────────────────────────────────────────────────

async fn list_nodes() -> Json<serde_json::Value> {
    let nodes = crate::k8s::list_nodes().await;
    Json(serde_json::json!({ "nodes": nodes }))
}
