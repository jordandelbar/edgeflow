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
    pub fn new(
        mut backend: Box<dyn InferenceBackend>,
        model_bytes: &[u8],
        pre_bytes: Option<&[u8]>,
        post_bytes: Option<&[u8]>,
    ) -> Result<Self> {
        tracing::info!("loading inference backend...");
        backend.load(model_bytes).context("failed to load model")?;
        tracing::info!("inference backend ready");

        let pre = pre_bytes.map(WasmTransform::new).transpose()?;
        let post = post_bytes.map(WasmTransform::new).transpose()?;

        Ok(Self { pre, backend, post })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        // Pre-process: raw bytes → tensor bytes
        let tensor_bytes = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None => raw_input.to_vec(),
        };

        // Decode flat binary tensor format → (shape, f32 data)
        let (shape, data) = tensor::decode(&tensor_bytes)?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");

        // Run backend
        let (out_shape, out_data) = self.backend.infer(&shape, &data)?;

        // Encode back to tensor bytes
        let output_tensor_bytes = tensor::encode(&out_shape, &out_data);

        // Post-process: tensor bytes → raw bytes
        match &mut self.post {
            Some(t) => t.run(&output_tensor_bytes),
            None => Ok(output_tensor_bytes),
        }
    }
}
