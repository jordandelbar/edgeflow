use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;

use crate::client::EdgeflowClient;
use crate::pipeline::Pipeline;
use crate::server::ModelInfo;

/// Instruction to load and activate a specific run.
pub struct DeployInstruction {
    pub run_id: String,
    pub deployment_id: String,
}

/// The live state of a loaded model. All three fields are updated atomically
/// by writing a new `Arc<ActiveDeployment>` under a single write lock, so
/// readers always see a consistent snapshot.
pub struct ActiveDeployment {
    pub pipeline: Arc<Mutex<Pipeline>>,
    pub model_info: ModelInfo,
    pub schema: Option<Vec<u8>>,
}

/// Blocking function: download artifacts, build new Pipeline, swap atomically.
/// Runs in a `spawn_blocking` thread so wasmtime and ORT are happy.
pub fn load_and_swap(
    req: DeployInstruction,
    shared_active: Arc<RwLock<Option<Arc<ActiveDeployment>>>>,
    client: Arc<EdgeflowClient>,
    target: String,
) {
    let rt = tokio::runtime::Handle::current();

    let result: Result<(Pipeline, Option<Vec<u8>>)> = rt
        .block_on(async {
            tracing::info!(run_id = %req.run_id, "downloading model.onnx");
            let model = client.download_artifact(&req.run_id, "model.onnx").await?;

            let pre_wasm = client
                .download_artifact(&req.run_id, "preprocess.wasm")
                .await
                .ok();
            let pre_cfg = client
                .download_artifact(&req.run_id, "preprocess.json")
                .await
                .ok();

            let post_wasm = client
                .download_artifact(&req.run_id, "postprocess.wasm")
                .await
                .ok();
            let post_cfg = client
                .download_artifact(&req.run_id, "postprocess.json")
                .await
                .ok();

            let schema = client
                .download_artifact(&req.run_id, "schema.json")
                .await
                .ok();

            Ok((model, pre_wasm, pre_cfg, post_wasm, post_cfg, schema))
        })
        .and_then(|(model, pre_wasm, pre_cfg, post_wasm, post_cfg, schema)| {
            let pre = pre_wasm.as_deref().map(|w| (w, pre_cfg.as_deref()));
            let post = post_wasm.as_deref().map(|w| (w, post_cfg.as_deref()));
            let backend = crate::backend::build_backend();
            let pipeline = Pipeline::new(backend, &model, pre, post, schema.as_deref())?;
            Ok((pipeline, schema))
        });

    match result {
        Ok((new_pipeline, schema)) => {
            *shared_active.write().unwrap() = Some(Arc::new(ActiveDeployment {
                pipeline: Arc::new(Mutex::new(new_pipeline)),
                model_info: ModelInfo {
                    run_id: req.run_id,
                    deployment_id: req.deployment_id.clone(),
                    target,
                    loaded_at: chrono::Utc::now().to_rfc3339(),
                },
                schema,
            }));

            tracing::info!(deployment_id = %req.deployment_id, "pipeline swapped successfully");

            let _ = rt.block_on(client.confirm_deployment(&req.deployment_id, "deployed", None));
        }
        Err(e) => {
            tracing::error!(deployment_id = %req.deployment_id, error = %e, "pipeline load failed");
            let _ = rt.block_on(client.confirm_deployment(
                &req.deployment_id,
                "failed",
                Some(&e.to_string()),
            ));
        }
    }
}
