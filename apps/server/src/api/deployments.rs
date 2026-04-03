use super::ApiError;
use crate::state::AppState;
use crate::target_client::TargetClient;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use edgeflow_core::{DeploymentState, InfraSettings, ResourceSettings};
use edgeflow_store::Store;
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/deployments", post(create_deployment))
        .route("/deployments", get(list_deployments))
        .route("/deployments/latest", get(get_latest_deployment))
        .route("/deployments/:id", get(get_deployment))
        .route("/deployments/:id/confirm", post(confirm_deployment))
        .route("/targets", get(list_targets))
        .route("/targets/register", post(register_target))
        .route("/targets/:target", get(get_target).delete(teardown_target))
        .route(
            "/targets/:target/resources",
            axum::routing::patch(update_target_resources),
        )
        .route("/targets/:target/model", get(target_model_status))
        .route("/targets/:target/schema", get(target_schema))
        .route("/targets/:target/health", get(target_health))
        .route(
            "/targets/:target/pods/:pod_id/heartbeat",
            post(target_heartbeat),
        )
        .route("/targets/:target/pending", get(target_pending))
        .route("/targets/:target/infer/playground", post(infer_playground))
        .route("/nodes", get(list_nodes))
}

async fn require_target(state: &AppState, target: &str) -> Result<edgeflow_core::Target, ApiError> {
    state
        .store
        .get_target(target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("target '{target}' not registered").into())
}

fn first_pod_address(target: &edgeflow_core::Target) -> Result<&str, ApiError> {
    target
        .pods
        .first()
        .map(|p| p.address.as_str())
        .ok_or_else(|| anyhow::anyhow!("target '{}' has no registered pods", target.target).into())
}

// ── GET /targets/:target/model ────────────────────────────────────────────────

async fn target_model_status(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = require_target(&state, &target).await?;

    let run_id = rec
        .current_run_id
        .ok_or_else(|| anyhow::anyhow!("no model loaded on target '{target}'"))?;
    let loaded_at = rec.model_loaded_at.unwrap_or_default();

    // Fetch the latest deployment id for this target for reference.
    let dep = state.store.get_latest_deployment(&target).await?;

    Ok(Json(serde_json::json!({
        "run_id":        run_id,
        "deployment_id": dep.deployment_id,
        "target":        target,
        "loaded_at":     loaded_at,
    })))
}

// ── GET /targets/:target/health ───────────────────────────────────────────────

async fn target_health(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = require_target(&state, &target).await?;
    let json = TargetClient::new(&state.http_client, first_pod_address(&rec)?)
        .health()
        .await?;
    Ok(Json(json))
}

// ── POST /targets/:target/pods/:pod_id/heartbeat ─────────────────────────────

async fn target_heartbeat(
    State(state): State<AppState>,
    Path((_target, pod_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.store.heartbeat_pod(&pod_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── GET /targets/:target/pending ──────────────────────────────────────────────

async fn target_pending(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let dep = state
        .store
        .get_pending_deployment_for_target(&target)
        .await?;
    let sessions = state
        .store
        .get_target(&target)
        .await?
        .and_then(|t| t.resources.sessions)
        .unwrap_or(1);
    Ok(Json(
        serde_json::json!({ "deployment": dep, "sessions": sessions }),
    ))
}

// ── GET /targets/:target/schema ───────────────────────────────────────────────

async fn target_schema(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rec = require_target(&state, &target).await?;
    let json = TargetClient::new(&state.http_client, first_pod_address(&rec)?)
        .schema()
        .await
        .map_err(|e| anyhow::anyhow!("no schema available on target '{target}': {e}"))?;
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
    let rec = require_target(&state, &target).await?;

    // Send raw packed floats — same format as the Python client (struct.pack('<Nf', ...)).
    // The preprocess WASM (FloatBytesToTensor) expects this, not a tensor-encoded header.
    let body: Vec<u8> = req.data.iter().flat_map(|&v| v.to_le_bytes()).collect();
    let infer_result = TargetClient::new(&state.http_client, first_pod_address(&rec)?)
        .infer(body)
        .await?;

    // The postprocess WASM can return anything. Try JSON first (ClassifierOutput,
    // custom transforms), then fall back to tensor decode (no postprocess).
    let result = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&infer_result) {
        json
    } else if let Ok((shape, data)) = edgeflow_common::tensor::decode(&infer_result) {
        serde_json::json!({ "shape": shape, "data": data })
    } else {
        return Err(anyhow::anyhow!("unrecognised response format from inference pod").into());
    };

    Ok(Json(result))
}

// ── POST /deployments ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateDeploymentRequest {
    model_name: String,
    model_version: String,
    target: String,
    node: Option<String>,
    /// Edgeflow-owned settings (sessions, max_concurrent).
    #[serde(default)]
    resources: ResourceSettings,
    /// k8s-owned infrastructure settings (cpu/memory/replicas/spread/node_selector).
    /// Passed directly to k8s; never stored in SQLite.
    #[serde(default)]
    infra: InfraSettings,
}

async fn create_deployment(
    State(state): State<AppState>,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let version_int: i64 = req.model_version.parse().map_err(|_| {
        anyhow::anyhow!(
            "model_version must be an integer, got '{}'",
            req.model_version
        )
    })?;
    let mv = state
        .store
        .get_model_version(&req.model_name, version_int)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "model version {} v{} not found",
                req.model_name,
                req.model_version
            )
        })?;
    let run_id = mv.run_id.ok_or_else(|| {
        anyhow::anyhow!(
            "model version {} v{} has no associated run",
            req.model_name,
            req.model_version
        )
    })?;

    let deployment = state
        .store
        .create_deployment(
            &run_id,
            &req.target,
            Some(&req.model_name),
            Some(&req.model_version),
        )
        .await?;

    // Check if pods are already registered for this target.
    if state.store.get_target(&req.target).await?.is_some() {
        // Upgrade path: pod is alive, tell it to load the new run.
        // If edgeflow resource settings were provided, persist them.
        if req.resources.sessions.is_some() || req.resources.max_concurrent.is_some() {
            state
                .store
                .store_target_resources(&req.target, &req.resources)
                .await?;
        }

        // Re-fetch to get the latest (possibly just-updated) resource settings.
        let sessions = state
            .store
            .get_target(&req.target)
            .await?
            .and_then(|t| t.resources.sessions)
            .unwrap_or(1) as usize;

        if let Some(ref publisher) = state.mqtt_publisher {
            // Mark Upgrading before publishing so pods can't confirm before
            // the state transition — eliminates a race with the local broker.
            state
                .store
                .update_deployment_state(&deployment.deployment_id, DeploymentState::Upgrading)
                .await?;

            if let Err(e) = publisher
                .publish_upgrade(
                    &req.target,
                    &deployment.run_id,
                    &deployment.deployment_id,
                    sessions,
                )
                .await
            {
                tracing::warn!(
                    deployment_id = %deployment.deployment_id,
                    error = %e,
                    "mqtt upgrade publish failed — deployment stays upgrading"
                );
            } else {
                tracing::info!(
                    deployment_id = %deployment.deployment_id,
                    target = %req.target,
                    "upgrade command published via MQTT"
                );
            }
        } else {
            tracing::warn!(
                deployment_id = %deployment.deployment_id,
                "no mqtt publisher available — deployment stays pending"
            );
        }
    } else {
        // First deploy: pod doesn't exist yet. Persist edgeflow-owned settings,
        // then create the k8s Deployment (k8s owns cpu/memory/replicas/etc.).
        let resolved_resources = crate::k8s::resolve_resources(&req.resources);
        state
            .store
            .store_target_resources(&req.target, &resolved_resources)
            .await?;
        crate::k8s::create_inference_pod(
            &req.target,
            req.node.as_deref(),
            &resolved_resources,
            &req.infra,
        )
        .await;
    }

    let deployment = state
        .store
        .get_deployment(&deployment.deployment_id)
        .await?;
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
    status: String, // "deployed" | "failed"
    reason: Option<String>,
}

async fn confirm_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deployment = state.store.get_deployment(&id).await?;

    // Idempotency guard: with multiple pods each sends its own confirm.
    // If already in a terminal state, return current state without modification.
    // We intentionally allow Pending (pod confirmed before server updated state —
    // race with local MQTT broker) as well as Deploying/Upgrading.
    if matches!(
        deployment.state,
        DeploymentState::Deployed | DeploymentState::Failed | DeploymentState::Superseded
    ) {
        return Ok(Json(serde_json::json!({ "deployment": deployment })));
    }

    match req.status.as_str() {
        "deployed" => {
            state
                .store
                .update_deployment_state(&id, DeploymentState::Deployed)
                .await?;
            // Record which model is now live on this target — server becomes the SSOT
            // so model info survives even if the inference pod is later torn down.
            let loaded_at = chrono::Utc::now().to_rfc3339();
            state
                .store
                .set_target_model(&deployment.target, &deployment.run_id, &loaded_at)
                .await?;
            // If this was an upgrade, supersede the previous deployed deployment.
            if deployment.state == DeploymentState::Upgrading {
                state
                    .store
                    .supersede_previous_deployments(&deployment.target, &id)
                    .await?;
            }
            tracing::info!(
                deployment_id = %id,
                target = %deployment.target,
                run_id = %deployment.run_id,
                "deployment confirmed deployed"
            );
        }
        "failed" => {
            state
                .store
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
    pod_id: String,
    address: String,
    node: Option<String>,
}

async fn register_target(
    State(state): State<AppState>,
    Json(req): Json<RegisterTargetRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let target = state
        .store
        .register_pod(&req.pod_id, &req.target, &req.address, req.node.as_deref())
        .await?;

    // Check for a pending deployment for this target — trigger the load.
    // Also handle the re-registration case (e.g. after a rolling restart
    // triggered by a resource update): if there is no pending deployment but
    // there IS a currently-deployed one, reload it on the new pod so it can
    // pass its readiness probe and let the old pod terminate.
    let pending = state
        .store
        .get_pending_deployment_for_target(&req.target)
        .await?;
    let deployment_to_load = if pending.is_some() {
        pending
    } else {
        // Look for the latest deployed deployment to reload after a restart.
        state
            .store
            .get_latest_deployment(&req.target)
            .await
            .ok()
            .filter(|d| d.state == DeploymentState::Deployed)
    };

    if let Some(deployment) = deployment_to_load {
        let is_reload = deployment.state == DeploymentState::Deployed;
        let sessions = target.resources.sessions.unwrap_or(1) as usize;
        match TargetClient::new(&state.http_client, &req.address)
            .upgrade(&deployment.run_id, &deployment.deployment_id, sessions)
            .await
        {
            Ok(true) => {
                if !is_reload {
                    state
                        .store
                        .update_deployment_state(
                            &deployment.deployment_id,
                            DeploymentState::Deploying,
                        )
                        .await?;
                }
                tracing::info!(
                    deployment_id = %deployment.deployment_id,
                    target = %req.target,
                    is_reload,
                    "triggered model load on newly registered pod"
                );
            }
            Ok(false) => {
                tracing::warn!("upgrade call to pod after registration returned non-success");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to reach newly registered pod");
            }
        }
    }

    Ok(Json(serde_json::json!({ "target": target })))
}

// ── PATCH /targets/:target/resources ─────────────────────────────────────────

#[derive(Deserialize, Default)]
struct UpdateResourcesRequest {
    /// Edgeflow-owned settings.
    #[serde(default)]
    resources: ResourceSettings,
    /// k8s-owned infrastructure settings — applied directly to the k8s Deployment.
    #[serde(default)]
    infra: InfraSettings,
}

async fn update_target_resources(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Json(req): Json<UpdateResourcesRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = require_target(&state, &target).await?;
    let prev = &existing.resources;

    let sessions_changed =
        req.resources.sessions.is_some() && req.resources.sessions != prev.sessions;
    let max_conc_changed = req.resources.max_concurrent.is_some()
        && req.resources.max_concurrent != prev.max_concurrent;
    let infra_changed = req.infra.cpu_request.is_some()
        || req.infra.memory_request.is_some()
        || req.infra.memory_limit.is_some()
        || req.infra.replicas.is_some()
        || req.infra.spread.is_some()
        || req.infra.node_selector.is_some();

    // Persist edgeflow-owned changes to SQLite.
    if sessions_changed || max_conc_changed {
        let merged = ResourceSettings {
            sessions: req.resources.sessions.or(prev.sessions),
            max_concurrent: req.resources.max_concurrent.or(prev.max_concurrent),
        };
        state.store.store_target_resources(&target, &merged).await?;
    }

    let mut pod_restarted = false;

    if !existing.pods.is_empty() {
        let pod_addr = existing
            .pods
            .first()
            .map(|p| p.address.clone())
            .unwrap_or_default();

        // Sessions: live-reload via upgrade — rebuilds the ORT pool without a
        // pod restart. max_concurrent requires restart (semaphore is fixed at startup).
        if sessions_changed {
            if let Some(run_id) = &existing.current_run_id {
                if let Ok(dep) = state.store.get_latest_deployment(&target).await {
                    let sessions = req.resources.sessions.unwrap_or(1) as usize;
                    let _ = TargetClient::new(&state.http_client, &pod_addr)
                        .upgrade(run_id, &dep.deployment_id, sessions)
                        .await;
                }
            }
        }

        // k8s infra changes and max_concurrent require a pod restart via k8s patch.
        if infra_changed || max_conc_changed {
            let res_patch = if max_conc_changed {
                Some(&req.resources)
            } else {
                None
            };
            let infra_patch = if infra_changed {
                Some(&req.infra)
            } else {
                None
            };
            pod_restarted =
                crate::k8s::patch_inference_pod_resources(&target, res_patch, infra_patch).await;
        }
    }

    let mut updated = require_target(&state, &target).await?;
    // Enrich with fresh k8s infra data.
    updated.infra = crate::k8s::get_inference_pod_infra(&target).await;
    Ok(Json(
        serde_json::json!({ "target": updated, "pod_restarted": pod_restarted }),
    ))
}

// ── GET /targets/:target ─────────────────────────────────────────────────────

async fn get_target(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Reconcile pod list against k8s — removes records for pods k8s has already terminated.
    if let Some(running) = crate::k8s::list_running_pod_names(&target).await {
        state.store.prune_pods(&target, &running).await?;
    }
    let mut t = require_target(&state, &target).await?;
    t.infra = crate::k8s::get_inference_pod_infra(&t.target).await;
    Ok(Json(serde_json::json!({ "target": t })))
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

async fn list_targets(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    // One k8s call to get all running pods, then prune stale records in bulk.
    if let Some(pods_by_target) = crate::k8s::list_all_running_pod_names().await {
        for (target, running) in &pods_by_target {
            let _ = state.store.prune_pods(target, running).await;
        }
        // Also prune pods for targets that have no running pods at all in k8s.
        let all_targets = state.store.list_targets().await?;
        for t in &all_targets {
            if !pods_by_target.contains_key(&t.target) {
                // k8s returned successfully but no pods for this target — all gone.
                let _ = state.store.prune_all_pods(&t.target).await;
            }
        }
    }

    let mut targets = state.store.list_targets().await?;
    for t in &mut targets {
        t.infra = crate::k8s::get_inference_pod_infra(&t.target).await;
    }
    Ok(Json(serde_json::json!({ "targets": targets })))
}

// ── GET /nodes ────────────────────────────────────────────────────────────────

async fn list_nodes() -> Json<serde_json::Value> {
    let nodes = crate::k8s::list_nodes().await;
    Json(serde_json::json!({ "nodes": nodes }))
}
