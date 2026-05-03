//! Wire-contract tests: pin the HTTP shape every `Api::*` method sends.
//!
//! The CLI and Python SDK both call into `edgeflow_client::Api`; if a method's
//! URL / verb / body silently changes, both surfaces drift from the server in
//! lockstep and the bug only surfaces at runtime. These tests stub the server
//! with `httpmock` and assert the recorded request matches the expected
//! contract. A failing test means: "the client now sends a request the server
//! doesn't (yet) understand - update the server endpoint, or revert the client."
//!
//! Once `Api` returns typed structs instead of `serde_json::Value` (tracked in
//! project #2: "client: replace serde_json::Value returns with typed structs")
//! these tests grow to assert response parsing too.
//!
//! Coverage: every public method on `Api` should have at least one test here.

use edgeflow_client::Api;
use httpmock::prelude::*;
use httpmock::Method::PATCH;
use serde_json::json;

fn api(server: &MockServer) -> Api {
    Api::new(&server.base_url())
}

// ── Experiments ──────────────────────────────────────────────────────────────

#[test]
fn list_experiments() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/2.0/mlflow/experiments/list");
        then.status(200).json_body(json!({"experiments": []}));
    });
    api(&server).list_experiments().unwrap();
    mock.assert();
}

#[test]
fn get_experiment() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/experiments/get")
            .query_param("experiment_id", "42");
        then.status(200).json_body(json!({"experiment": {}}));
    });
    api(&server).get_experiment("42").unwrap();
    mock.assert();
}

#[test]
fn get_experiment_by_name() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/experiments/get-by-name")
            .query_param("experiment_name", "my-exp");
        then.status(200).json_body(json!({"experiment": {}}));
    });
    api(&server).get_experiment_by_name("my-exp").unwrap();
    mock.assert();
}

// ── Runs ─────────────────────────────────────────────────────────────────────

#[test]
fn search_runs() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/runs/search")
            .json_body(json!({"experiment_ids": ["7"]}));
        then.status(200).json_body(json!({"runs": []}));
    });
    api(&server).search_runs("7").unwrap();
    mock.assert();
}

#[test]
fn get_run() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/runs/get")
            .query_param("run_id", "abc123");
        then.status(200).json_body(json!({"run": {}}));
    });
    api(&server).get_run("abc123").unwrap();
    mock.assert();
}

// ── Model Registry ───────────────────────────────────────────────────────────

#[test]
fn list_registered_models() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/registered-models/list");
        then.status(200).json_body(json!({"registered_models": []}));
    });
    api(&server).list_registered_models().unwrap();
    mock.assert();
}

#[test]
fn list_model_versions() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/model-versions/search")
            .json_body(json!({"filter": "name = 'iris-classifier'"}));
        then.status(200).json_body(json!({"model_versions": []}));
    });
    api(&server).list_model_versions("iris-classifier").unwrap();
    mock.assert();
}

#[test]
fn register_model_creates_then_versions() {
    // register_model is a two-call composite: create the registered model
    // (idempotent, ignored on failure), then create a version.
    let server = MockServer::start();
    let create = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/registered-models/create")
            .json_body(json!({"name": "iris"}));
        then.status(200).json_body(json!({}));
    });
    let version = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/model-versions/create")
            .json_body(json!({"name": "iris", "run_id": "run-xyz"}));
        then.status(200).json_body(json!({"model_version": {}}));
    });
    api(&server).register_model("run-xyz", "iris").unwrap();
    create.assert();
    version.assert();
}

#[test]
fn transition_stage() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/model-versions/transition-stage")
            .json_body(json!({
                "name": "iris",
                "version": "3",
                "stage": "Production",
            }));
        then.status(200).json_body(json!({"model_version": {}}));
    });
    api(&server)
        .transition_stage("iris", "3", "Production")
        .unwrap();
    mock.assert();
}

#[test]
fn delete_registered_model() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/registered-models/delete")
            .json_body(json!({"name": "iris"}));
        then.status(200).json_body(json!({}));
    });
    api(&server).delete_registered_model("iris").unwrap();
    mock.assert();
}

#[test]
fn delete_model_version() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/2.0/mlflow/model-versions/delete")
            .json_body(json!({"name": "iris", "version": "2"}));
        then.status(200).json_body(json!({}));
    });
    api(&server).delete_model_version("iris", "2").unwrap();
    mock.assert();
}

// ── Deployments ──────────────────────────────────────────────────────────────

#[test]
fn create_deployment() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/deployments")
            .json_body(json!({
                "model_name": "iris",
                "model_version": "3",
                "target": "iris-inference",
                "resources": {"sessions": 4, "max_concurrent": 8},
            }));
        then.status(200).json_body(json!({"deployment": {}}));
    });
    api(&server)
        .create_deployment("iris", "3", "iris-inference", Some(4), Some(8))
        .unwrap();
    mock.assert();
}

#[test]
fn create_deployment_with_null_resources() {
    // When sessions/max_concurrent are None they must serialize as JSON null,
    // not be omitted. The server defaults them; omitting would change shape.
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/v1/deployments")
            .json_body(json!({
                "model_name": "iris",
                "model_version": "3",
                "target": "iris-inference",
                "resources": {"sessions": null, "max_concurrent": null},
            }));
        then.status(200).json_body(json!({"deployment": {}}));
    });
    api(&server)
        .create_deployment("iris", "3", "iris-inference", None, None)
        .unwrap();
    mock.assert();
}

#[test]
fn list_deployments_no_filter() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/deployments");
        then.status(200).json_body(json!({"deployments": []}));
    });
    api(&server).list_deployments(None).unwrap();
    mock.assert();
}

#[test]
fn list_deployments_filtered_by_target() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/deployments")
            .query_param("target", "iris-inference");
        then.status(200).json_body(json!({"deployments": []}));
    });
    api(&server)
        .list_deployments(Some("iris-inference"))
        .unwrap();
    mock.assert();
}

#[test]
fn get_deployment() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/deployments/dep-abc-123");
        then.status(200).json_body(json!({"deployment": {}}));
    });
    api(&server).get_deployment("dep-abc-123").unwrap();
    mock.assert();
}

#[test]
fn latest_deployment() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/deployments/latest")
            .query_param("target", "iris-inference");
        then.status(200).json_body(json!({"deployment": {}}));
    });
    api(&server).latest_deployment("iris-inference").unwrap();
    mock.assert();
}

// ── Targets ──────────────────────────────────────────────────────────────────

#[test]
fn list_targets() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/targets");
        then.status(200).json_body(json!({"targets": []}));
    });
    api(&server).list_targets().unwrap();
    mock.assert();
}

#[test]
fn get_target() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/targets/iris-inference");
        then.status(200).json_body(json!({"target": {}}));
    });
    api(&server).get_target("iris-inference").unwrap();
    mock.assert();
}

#[test]
fn update_target_resources_full() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PATCH)
            .path("/api/v1/targets/iris-inference/resources")
            .json_body(json!({
                "resources": {"sessions": 4, "max_concurrent": 8},
                "infra": {
                    "cpu_request": "500m",
                    "memory_request": "256Mi",
                    "memory_limit": "512Mi",
                    "replicas": 2,
                    "placement": "spread",
                },
            }));
        then.status(200).json_body(json!({"target": {}}));
    });
    api(&server)
        .update_target_resources(
            "iris-inference",
            Some(4),
            Some(8),
            Some("500m"),
            Some("256Mi"),
            Some("512Mi"),
            Some(2),
            Some("spread"),
        )
        .unwrap();
    mock.assert();
}

#[test]
fn update_target_resources_all_nones() {
    // None for every field should send JSON nulls, not omit the keys -
    // the server's PATCH semantics rely on the structure being present.
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PATCH)
            .path("/api/v1/targets/iris-inference/resources")
            .json_body(json!({
                "resources": {"sessions": null, "max_concurrent": null},
                "infra": {
                    "cpu_request": null,
                    "memory_request": null,
                    "memory_limit": null,
                    "replicas": null,
                    "placement": null,
                },
            }));
        then.status(200).json_body(json!({"target": {}}));
    });
    api(&server)
        .update_target_resources("iris-inference", None, None, None, None, None, None, None)
        .unwrap();
    mock.assert();
}

#[test]
fn teardown_target() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(DELETE).path("/api/v1/targets/iris-inference");
        then.status(204);
    });
    api(&server).teardown_target("iris-inference").unwrap();
    mock.assert();
}

// ── Nodes ────────────────────────────────────────────────────────────────────

#[test]
fn list_nodes() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/nodes");
        then.status(200).json_body(json!({"nodes": []}));
    });
    api(&server).list_nodes().unwrap();
    mock.assert();
}

// ── Resolvers ────────────────────────────────────────────────────────────────
// resolve_experiment / resolve_run_id are composites of the methods above.
// Each underlying call is already pinned; we only verify the composition order
// (try-by-name then fall back to by-id) so a refactor doesn't silently flip it.

#[test]
fn resolve_experiment_prefers_by_name() {
    let server = MockServer::start();
    let by_name = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/experiments/get-by-name")
            .query_param("experiment_name", "iris");
        then.status(200).json_body(json!({"experiment": {}}));
    });
    let by_id = server.mock(|when, then| {
        when.method(GET).path("/api/2.0/mlflow/experiments/get");
        then.status(200).json_body(json!({"experiment": {}}));
    });
    api(&server).resolve_experiment("iris").unwrap();
    by_name.assert();
    assert_eq!(
        by_id.calls(),
        0,
        "should not fall back when name lookup succeeds"
    );
}

#[test]
fn resolve_experiment_falls_back_to_id() {
    let server = MockServer::start();
    let by_name = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/experiments/get-by-name");
        then.status(404).json_body(json!({}));
    });
    let by_id = server.mock(|when, then| {
        when.method(GET)
            .path("/api/2.0/mlflow/experiments/get")
            .query_param("experiment_id", "42");
        then.status(200).json_body(json!({"experiment": {}}));
    });
    api(&server).resolve_experiment("42").unwrap();
    by_name.assert();
    by_id.assert();
}

#[test]
fn resolve_run_id_passes_through_full_id() {
    // Full 32-char ids skip the network entirely.
    let server = MockServer::start();
    let any_call = server.mock(|when, then| {
        when.method(GET).path_prefix("/api/");
        then.status(500);
    });
    let full_id = "0123456789abcdef0123456789abcdef";
    let resolved = api(&server).resolve_run_id(full_id).unwrap();
    assert_eq!(resolved, full_id);
    assert_eq!(
        any_call.calls(),
        0,
        "full-length id must not hit the network"
    );
}
