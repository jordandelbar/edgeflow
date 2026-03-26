use anyhow::{Context, Result};

use crate::backend::InferenceBackend;
use crate::tensor;
use crate::wasm::WasmTransform;

pub struct Pipeline {
    pre: Option<WasmTransform>,
    backend: Box<dyn InferenceBackend>,
    post: Option<WasmTransform>,
}

impl Pipeline {
    /// Build a pipeline from raw artifact bytes.
    ///
    /// `pre` and `post` are `(wasm_bytes, config_bytes)` pairs.  Config bytes
    /// are `Some` for standard Rust pipelines (triggers `init`) and `None` for
    /// legacy componentize-py components.
    pub fn new(
        mut backend: Box<dyn InferenceBackend>,
        model_bytes: &[u8],
        pre: Option<(&[u8], Option<&[u8]>)>,
        post: Option<(&[u8], Option<&[u8]>)>,
    ) -> Result<Self> {
        tracing::info!("loading inference backend...");
        backend.load(model_bytes).context("failed to load model")?;
        tracing::info!("inference backend ready");

        let pre = pre
            .map(|(w, c)| WasmTransform::new(w, c))
            .transpose()
            .context("failed to load preprocess transform")?;
        let post = post
            .map(|(w, c)| WasmTransform::new(w, c))
            .transpose()
            .context("failed to load postprocess transform")?;

        Ok(Self { pre, backend, post })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let tensor_bytes = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None => raw_input.to_vec(),
        };

        let (shape, data) = tensor::decode(&tensor_bytes)?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");

        let (out_shape, out_data) = self.backend.infer(&shape, &data)?;
        let output_tensor_bytes = tensor::encode(&out_shape, &out_data);

        match &mut self.post {
            Some(t) => t.run(&output_tensor_bytes),
            None => Ok(output_tensor_bytes),
        }
    }
}
