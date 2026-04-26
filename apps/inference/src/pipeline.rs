use std::borrow::Cow;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::backend::InferenceBackend;
use crate::inputs::{self, InputMode, InputSpec};
use crate::tensor;
use crate::wasm::WasmTransform;

/// Pre-transform step types that convert raw input bytes into wire-format
/// tensor bytes. When one of these is at the head of the pre-pipeline, the
/// host can ask the WASM to start at step 1 on JSON-array inputs (which
/// already arrive as tensors) so every following preprocessor still runs.
const FORMAT_ADAPTERS: &[&str] = &["float_to_tensor", "image_to_tensor"];

pub struct Pipeline {
    /// Pre-transform component. Single instance; the host calls
    /// `run_from(bytes, pre_adapter_offset)` to skip the format adapter on
    /// the JSON-array path without re-instantiating.
    pre: Option<WasmTransform>,
    /// 1 when `pre` starts with a known format adapter (so JSON-array inputs
    /// can skip it via `transform-from`), 0 otherwise. Always 0 for legacy
    /// componentize-py components - they don't expose step-level indexing.
    pre_adapter_offset: u32,
    /// True when the pre-transform is a legacy componentize-py component
    /// (opaque config). Preserves the original "JSON bypasses pre entirely"
    /// behavior for back-compat.
    pre_is_legacy: bool,
    backend: Box<dyn InferenceBackend>,
    post: Option<WasmTransform>,
    /// Determined from schema.json at load time.
    input_mode: InputMode,
    /// Non-empty only for Named mode.
    specs: Vec<InputSpec>,
    /// Reused across requests for the JSON-array and Named encode steps.
    input_buf: Vec<u8>,
    /// Reused across requests for backend output (wire-format bytes).
    output_buf: Vec<u8>,
}

/// Inspect a pre-transform config and return 1 when `steps[0]` is a known
/// format adapter, 0 otherwise. The returned offset is fed into
/// `WasmTransform::run_from` on the JSON-array path.
fn detect_adapter_offset(config: Option<&[u8]>) -> Result<u32> {
    let Some(cfg_bytes) = config else {
        return Ok(0);
    };
    let parsed: Value = serde_json::from_slice(cfg_bytes)
        .context("failed to parse pre-transform config as JSON")?;
    let Some(steps) = parsed.get("steps").and_then(Value::as_array) else {
        return Ok(0);
    };
    let first_is_adapter = steps
        .first()
        .and_then(|s| s.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|t| FORMAT_ADAPTERS.contains(&t));
    Ok(if first_is_adapter { 1 } else { 0 })
}

impl Pipeline {
    /// Build a pipeline from raw artifact bytes.
    ///
    /// `pre` and `post` are `(wasm_bytes, config_bytes)` pairs. Config bytes
    /// are `Some` for standard Rust pipelines (triggers `init`) and `None` for
    /// legacy componentize-py components.
    ///
    /// `schema` is the raw bytes of `schema.json`, used to determine whether
    /// the model expects raw f32 bytes (Single) or a JSON body (Named).
    ///
    /// A single wasmtime Engine is shared between every WASM transform built
    /// here, so JIT resources (compiled code cache) are initialized once.
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

        let (pre_transform, pre_adapter_offset, pre_is_legacy, post) =
            if pre.is_some() || post.is_some() {
                let engine = WasmTransform::build_engine()?;
                let (pre_transform, pre_adapter_offset, pre_is_legacy) =
                    if let Some((wasm, cfg)) = pre {
                        let offset = detect_adapter_offset(cfg)?;
                        let is_legacy = cfg.is_none();
                        let t = WasmTransform::new(&engine, wasm, cfg)
                            .context("failed to load preprocess transform")?;
                        (Some(t), offset, is_legacy)
                    } else {
                        (None, 0, false)
                    };
                let post = post
                    .map(|(w, c)| WasmTransform::new(&engine, w, c))
                    .transpose()
                    .context("failed to load postprocess transform")?;
                (pre_transform, pre_adapter_offset, pre_is_legacy, post)
            } else {
                (None, 0, false, None)
            };

        let (input_mode, specs) = inputs::parse_schema(schema);
        tracing::info!(mode = ?input_mode, "pipeline input mode");

        Ok(Self {
            pre: pre_transform,
            pre_adapter_offset,
            pre_is_legacy,
            backend,
            post,
            input_mode,
            specs,
            input_buf: Vec::new(),
            output_buf: Vec::new(),
        })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let json_array_path =
            self.input_mode == InputMode::Single && inputs::looks_like_json_array(raw_input);

        // ── Stage 1+2: run pre-transform. ──
        //
        // Single binary:    pre from step 0.
        // Single JSON array: parsed JSON encoded into self.input_buf; standard
        //                    pre runs from `pre_adapter_offset` to skip the
        //                    format adapter; legacy pre (opaque config) is
        //                    bypassed.
        // Named:            JSON object encoded into self.input_buf; pre is
        //                    bypassed.
        let pre_output: Cow<'_, [u8]> = if json_array_path {
            let (shape, data) = {
                let _span = tracing::info_span!("tensor.decode").entered();
                inputs::json_array_to_tensor(raw_input)
                    .context("failed to decode JSON array input")?
            };
            tensor::encode_into(&shape, &data, &mut self.input_buf);
            match (&mut self.pre, self.pre_is_legacy) {
                (Some(t), false) => {
                    let _span = tracing::info_span!("wasm.preprocess").entered();
                    Cow::Owned(t.run_from(&self.input_buf, self.pre_adapter_offset)?)
                }
                _ => Cow::Borrowed(self.input_buf.as_slice()),
            }
        } else if self.input_mode == InputMode::Named {
            let _span = tracing::info_span!("tensor.decode").entered();
            let (shape, data) = inputs::json_to_tensor(raw_input, &self.specs)
                .context("failed to encode JSON input to tensor")?;
            tensor::encode_into(&shape, &data, &mut self.input_buf);
            Cow::Borrowed(self.input_buf.as_slice())
        } else {
            match &mut self.pre {
                Some(t) => {
                    let _span = tracing::info_span!("wasm.preprocess").entered();
                    Cow::Owned(t.run(raw_input)?)
                }
                None => Cow::Borrowed(raw_input),
            }
        };

        // ── Stage 3: decode wire format and run backend. ──
        let (shape, data) = tensor::decode(pre_output.as_ref())?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");
        {
            let _span = tracing::info_span!("backend.infer").entered();
            self.backend.infer(&shape, data, &mut self.output_buf)?;
        }

        // ── Stage 4: post-transform. Output is already wire-format in output_buf. ──
        match &mut self.post {
            Some(t) => {
                let _span = tracing::info_span!("wasm.postprocess").entered();
                t.run(&self.output_buf)
            }
            None => Ok(std::mem::take(&mut self.output_buf)),
        }
    }
}
