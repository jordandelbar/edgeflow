use anyhow::{Context, Result};
use std::borrow::Cow;

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
    ///
    /// A single wasmtime Engine is shared between the pre- and post-transforms,
    /// so JIT resources (thread pools, code cache) are initialized only once.
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

        Ok(Self {
            pre,
            backend,
            post,
            input_mode,
            specs,
        })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        // Per-request format dispatch: a JSON array body in Single mode bypasses
        // the pre-transform entirely (the transform assumes binary tensor input
        // and would mangle a JSON body), going straight to the array decoder.
        let json_array_path =
            self.input_mode == InputMode::Single && inputs::looks_like_json_array(raw_input);

        // Apply pre-transform only on the binary/Named paths. `body` must live
        // past the decode match so `Cow::Borrowed` can hold a slice into it on
        // the zero-copy Single binary path.
        let body: Vec<u8> = if json_array_path {
            Vec::new()
        } else {
            match &mut self.pre {
                Some(t) => {
                    let _span = tracing::info_span!("wasm.preprocess").entered();
                    t.run(raw_input)?
                }
                None => raw_input.to_vec(),
            }
        };

        // Parse into a flat f32 tensor.
        //
        // Single binary: decode returns a &[f32] view directly into `body` - no allocation.
        // Single JSON array: per-request body sniff; allocates a Vec<f32>.
        // Named:  json_to_tensor always allocates a Vec<f32> (field-by-field build).
        let (shape, data): (Vec<usize>, Cow<'_, [f32]>) = {
            let _span = tracing::info_span!("tensor.decode").entered();
            if json_array_path {
                let (shape, data) = inputs::json_array_to_tensor(raw_input)
                    .context("failed to decode JSON array input")?;
                (shape, Cow::Owned(data))
            } else {
                match self.input_mode {
                    InputMode::Single => {
                        let (shape, data) = tensor::decode(&body)?;
                        let n: usize = shape.iter().product();
                        anyhow::ensure!(data.len() == n, "tensor data length mismatch");
                        (shape, Cow::Borrowed(data))
                    }
                    InputMode::Named => {
                        let (shape, data) = inputs::json_to_tensor(&body, &self.specs)
                            .context("failed to encode JSON input to tensor")?;
                        (shape, Cow::Owned(data))
                    }
                }
            }
        };

        // Run inference backend.
        let (out_shape, out_data) = {
            let _span = tracing::info_span!("backend.infer").entered();
            self.backend.infer(&shape, &data)?
        };

        let output_tensor_bytes = {
            let _span = tracing::info_span!("tensor.encode").entered();
            tensor::encode(&out_shape, &out_data)
        };

        // Apply post-transform (if any).
        match &mut self.post {
            Some(t) => {
                let _span = tracing::info_span!("wasm.postprocess").entered();
                t.run(&output_tensor_bytes)
            }
            None => Ok(output_tensor_bytes),
        }
    }
}
