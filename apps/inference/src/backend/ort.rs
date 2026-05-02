use anyhow::{Context, Result};
use ort::session::Session;
use ort::value::TensorRef;

use super::InferenceBackend;
use edgeflow_common::tensor;

pub struct OrtBackend {
    session: Option<Session>,
}

impl OrtBackend {
    pub fn new() -> Self {
        Self { session: None }
    }
}

impl Default for OrtBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceBackend for OrtBackend {
    fn load(&mut self, model_bytes: &[u8]) -> Result<()> {
        let session = Session::builder()
            .context("failed to create ORT session builder")?
            .commit_from_memory(model_bytes)
            .context("failed to load ONNX model into ORT session")?;
        self.session = Some(session);
        Ok(())
    }

    fn infer(&mut self, shape: &[usize], data: &[f32], out: &mut Vec<u8>) -> Result<()> {
        let session = self.session.as_mut().context("model not loaded")?;

        let ort_shape: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
        // TensorRef borrows `data` directly - no copy of the input floats.
        let tensor = TensorRef::<f32>::from_array_view((ort_shape.as_slice(), data))
            .context("failed to build ORT input tensor")?;

        let outputs = session
            .run(ort::inputs![tensor])
            .context("ORT inference failed")?;

        let (out_shape, out_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("failed to extract f32 output from ORT")?;

        let out_shape_usize: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
        tensor::encode_into(&out_shape_usize, out_data, out);
        Ok(())
    }
}
