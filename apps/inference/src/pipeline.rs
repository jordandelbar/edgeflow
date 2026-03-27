use anyhow::{Context, Result};

use crate::backend::InferenceBackend;
use crate::inputs::{self, InputMode, InputSpec};
use crate::tensor;
use crate::wasm::WasmTransform;

pub struct Pipeline {
    pre: Option<WasmTransform>,
    backend: Box<dyn InferenceBackend>,
    post: Option<WasmTransform>,
    /// Determined from schema.json at load time.
    input_mode: InputMode,
    /// Non-empty only for Named mode.
    specs: Vec<InputSpec>,
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
    pub fn new(
        mut backend: Box<dyn InferenceBackend>,
        model_bytes: &[u8],
        pre: Option<(&[u8], Option<&[u8]>)>,
        post: Option<(&[u8], Option<&[u8]>)>,
        schema: Option<&[u8]>,
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

        let (input_mode, specs) = inputs::parse_schema(schema);
        tracing::info!(mode = ?input_mode, "pipeline input mode");

        Ok(Self {
            pre,
            backend,
            post,
            input_mode,
            specs,
        })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let output_tensor_bytes = match self.input_mode {
            InputMode::Single => self.infer_single(raw_input)?,
            InputMode::Named => self.infer_named(raw_input)?,
        };

        match &mut self.post {
            Some(t) => t.run(&output_tensor_bytes),
            None => Ok(output_tensor_bytes),
        }
    }

    /// Existing path: WASM preprocess → f32 wire decode → backend → f32 wire encode.
    fn infer_single(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let tensor_bytes = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None => raw_input.to_vec(),
        };

        let (shape, data) = tensor::decode(&tensor_bytes)?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");

        let (out_shape, out_data) = self.backend.infer(&shape, &data)?;
        Ok(tensor::encode(&out_shape, &out_data))
    }

    /// New path: optional WASM preprocess (protocol adapter) → JSON parse +
    /// categorical encoding → f32 tensor → backend → f32 wire encode.
    ///
    /// The WASM preprocess hook is still honoured here so edge devices with
    /// non-JSON binary protocols can adapt their payload before encoding.
    fn infer_named(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let body = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None => raw_input.to_vec(),
        };

        let (shape, data) = inputs::json_to_tensor(&body, &self.specs)
            .context("failed to encode JSON input to tensor")?;

        let (out_shape, out_data) = self.backend.infer(&shape, &data)?;
        Ok(tensor::encode(&out_shape, &out_data))
    }
}
