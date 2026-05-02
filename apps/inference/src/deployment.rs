use std::sync::{Arc, Condvar, Mutex, RwLock};

use anyhow::{anyhow, Result};

use edgeflow_inference::pipeline::Pipeline;

use crate::client::EdgeflowClient;
use crate::server::ModelInfo;

/// Instruction to load and activate a specific run.
pub struct DeployInstruction {
    pub run_id: String,
    pub deployment_id: String,
    /// Number of ORT sessions to create - provided by the server so sessions
    /// can change on each hot-swap without requiring a pod restart.
    pub sessions: usize,
}

/// Pool of Pipeline instances.  Checkout blocks (in a spawn_blocking thread)
/// until a pipeline is free; checkin wakes one waiter.  Pool size equals the
/// number of ORT sessions created at load time.
pub struct PipelinePool {
    inner: Mutex<Vec<Pipeline>>,
    cvar: Condvar,
}

impl PipelinePool {
    fn new(pipelines: Vec<Pipeline>) -> Self {
        Self {
            inner: Mutex::new(pipelines),
            cvar: Condvar::new(),
        }
    }

    pub fn checkout(&self) -> Pipeline {
        let mut guard = self.inner.lock().unwrap();
        loop {
            if let Some(p) = guard.pop() {
                return p;
            }
            guard = self.cvar.wait(guard).unwrap();
        }
    }

    pub fn checkin(&self, pipeline: Pipeline) {
        self.inner.lock().unwrap().push(pipeline);
        self.cvar.notify_one();
    }
}

/// The live state of a loaded model. All three fields are updated atomically
/// by writing a new `Arc<ActiveDeployment>` under a single write lock, so
/// readers always see a consistent snapshot.
pub struct ActiveDeployment {
    pub pool: Arc<PipelinePool>,
    pub model_info: ModelInfo,
    pub schema: Option<Vec<u8>>,
}

/// Blocking function: download artifacts, build `req.sessions` Pipelines, swap atomically.
/// Runs in a `spawn_blocking` thread so wasmtime and ORT are happy.
pub fn load_and_swap(
    req: DeployInstruction,
    shared_active: Arc<RwLock<Option<Arc<ActiveDeployment>>>>,
    client: Arc<EdgeflowClient>,
    target: Arc<str>,
) {
    let sessions = req.sessions;
    let rt = tokio::runtime::Handle::current();

    let result: Result<(Vec<Pipeline>, Option<Vec<u8>>)> = rt
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
            let pre = match (pre_wasm.as_deref(), pre_cfg.as_deref()) {
                (Some(w), Some(c)) => Some((w, c)),
                (Some(_), None) => {
                    return Err(anyhow!(
                        "preprocess.wasm present but preprocess.json missing"
                    ))
                }
                (None, _) => None,
            };
            let post = match (post_wasm.as_deref(), post_cfg.as_deref()) {
                (Some(w), Some(c)) => Some((w, c)),
                (Some(_), None) => {
                    return Err(anyhow!(
                        "postprocess.wasm present but postprocess.json missing"
                    ))
                }
                (None, _) => None,
            };

            tracing::info!(sessions, "building session pool");
            let mut pipelines = Vec::with_capacity(sessions);
            for i in 0..sessions {
                tracing::info!(session = i + 1, sessions, "loading session");
                let backend = edgeflow_inference::backend::build_backend();
                let pipeline = Pipeline::new(backend, &model, pre, post, schema.as_deref())?;
                pipelines.push(pipeline);
            }
            Ok((pipelines, schema))
        });

    match result {
        Ok((pipelines, schema)) => {
            *shared_active.write().unwrap() = Some(Arc::new(ActiveDeployment {
                pool: Arc::new(PipelinePool::new(pipelines)),
                model_info: ModelInfo {
                    run_id: req.run_id,
                    deployment_id: req.deployment_id.clone(),
                    target: target.to_string(),
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
