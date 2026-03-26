use anyhow::Result;

#[cfg(feature = "ort-backend")]
pub mod ort;
#[cfg(feature = "tract-backend")]
pub mod tract;

/// Trait implemented by each inference backend (ORT, tract, etc.).
pub trait InferenceBackend: Send + Sync {
    /// Load a model from raw ONNX bytes.
    fn load(&mut self, model_bytes: &[u8]) -> Result<()>;

    /// Run inference on a decoded tensor.
    ///
    /// Receives the input as `(shape, flat f32 data)` and returns the output
    /// in the same form.  The pipeline handles shape/dtype encoding/decoding;
    /// the backend only deals with f32 tensors.
    fn infer(&mut self, shape: &[usize], data: &[f32]) -> Result<(Vec<usize>, Vec<f32>)>;
}
