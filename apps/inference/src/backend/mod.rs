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
    fn load(&mut self, model_bytes: &[u8]) -> Result<()>;

    /// Run inference on a single f32 tensor.
    ///
    /// Input is `(shape, flat f32 data)`. Output is written into `out` as
    /// wire-format bytes (see `edgeflow_common::tensor::encode_into`). The
    /// caller owns `out` and reuses it across requests for allocator pooling.
    fn infer(&self, shape: &[usize], data: &[f32], out: &mut Vec<u8>) -> Result<()>;
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
