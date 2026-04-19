use anyhow::Result;

#[cfg(feature = "ort-backend")]
pub mod ort;
#[cfg(feature = "tract-backend")]
pub mod tract;

/// Trait implemented by each inference backend (ORT, tract, etc.).
///
/// The backend always receives a decoded f32 tensor. JSON parsing and
/// categorical encoding for Named-mode models happen in the pipeline layer
/// using the encoding tables stored in schema.json.
pub trait InferenceBackend: Send + Sync {
    /// Load a model from raw ONNX bytes.
    fn load(&mut self, model_bytes: &[u8]) -> Result<()>;

    /// Run inference on a single f32 tensor.
    ///
    /// Receives the input as `(shape, flat f32 data)` and returns the output
    /// in the same form.
    fn infer(&mut self, shape: &[usize], data: &[f32]) -> Result<(Vec<usize>, Vec<f32>)>;
}

/// Construct the configured backend at runtime.
pub fn build_backend() -> Box<dyn InferenceBackend> {
    #[cfg(feature = "ort-backend")]
    {
        tracing::info!("using ORT backend");
        Box::new(ort::OrtBackend::new())
    }
    #[cfg(feature = "tract-backend")]
    {
        tracing::info!("using tract backend");
        Box::new(tract::TractBackend::new())
    }
    #[cfg(not(any(feature = "ort-backend", feature = "tract-backend")))]
    compile_error!("at least one inference backend feature must be enabled");
}
