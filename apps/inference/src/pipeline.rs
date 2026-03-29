use anyhow::{Context, Result};

use crate::backend::InferenceBackend;
use crate::inputs::{self, InputMode, InputSpec};
use crate::tensor;
use crate::wasm::WasmTransform;

pub struct Pipeline {
    pre:        Option<WasmTransform>,
    backend:    Box<dyn InferenceBackend>,
    post:       Option<WasmTransform>,
    /// Determined from schema.json at load time.
    input_mode: InputMode,
    /// Non-empty only for Named mode.
    specs:      Vec<InputSpec>,
}

impl Pipeline {
    /// Build a pipeline from raw artifact bytes.
    ///
    /// `pre` and `post` are `(wasm_bytes, config_bytes)` pairs.  Config bytes
    /// are `Some` for standard Rust pipelines (triggers `init`) and `None` for
    /// legacy componentize-py components.
    ///
    /// `schema` is the raw bytes of `schema.json`, used to determine whether
    /// the model expects raw f32 bytes (Single) or a JSON body (Named).
    ///
    /// A single wasmtime Engine is shared between the pre and post transforms
    /// so JIT resources (thread pools, code cache) are initialised only once.
    pub fn new(
        mut backend: Box<dyn InferenceBackend>,
        model_bytes: &[u8],
        pre:  Option<(&[u8], Option<&[u8]>)>,
        post: Option<(&[u8], Option<&[u8]>)>,
        schema: Option<&[u8]>,
    ) -> Result<Self> {
        tracing::info!("loading inference backend...");
        backend.load(model_bytes).context("failed to load model")?;
        tracing::info!("inference backend ready");

        let (pre, post) = if pre.is_some() || post.is_some() {
            let engine = WasmTransform::build_engine()?;
            let pre = pre
                .map(|(w, c)| WasmTransform::new(&engine, w, c))
                .transpose()
                .context("failed to load preprocess transform")?;
            let post = post
                .map(|(w, c)| WasmTransform::new(&engine, w, c))
                .transpose()
                .context("failed to load postprocess transform")?;
            (pre, post)
        } else {
            (None, None)
        };

        let (input_mode, specs) = inputs::parse_schema(schema);
        tracing::info!(mode = ?input_mode, "pipeline input mode");

        Ok(Self { pre, backend, post, input_mode, specs })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        // Apply pre-transform (if any).
        let body = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None    => raw_input.to_vec(),
        };

        // Parse body into a flat f32 tensor according to input mode.
        let (shape, data) = match self.input_mode {
            InputMode::Single => {
                let (shape, data) = tensor::decode(&body)?;
                let n: usize = shape.iter().product();
                anyhow::ensure!(data.len() == n, "tensor data length mismatch");
                (shape, data)
            }
            InputMode::Named => {
                inputs::json_to_tensor(&body, &self.specs)
                    .context("failed to encode JSON input to tensor")?
            }
        };

        // Run inference backend.
        let (out_shape, out_data) = self.backend.infer(&shape, &data)?;
        let output_tensor_bytes = tensor::encode(&out_shape, &out_data);

        // Apply post-transform (if any).
        match &mut self.post {
            Some(t) => t.run(&output_tensor_bytes),
            None    => Ok(output_tensor_bytes),
        }
    }
}
