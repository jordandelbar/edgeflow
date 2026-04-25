use anyhow::{Context, Result};
use serde_json::{json, Value};
use wasmtime::Engine;

use crate::backend::InferenceBackend;
use crate::inputs::{self, InputMode, InputSpec};
use crate::tensor;
use crate::wasm::WasmTransform;

/// Pre-transform step types that convert raw input bytes into wire-format
/// tensor bytes. They get split off as the "decoder" stage so JSON-array
/// inputs (which already arrive as tensors) can skip them while still
/// running every preprocessor that follows.
const FORMAT_ADAPTERS: &[&str] = &["float_to_tensor", "image_to_tensor"];

pub struct Pipeline {
    /// Format adapter step (FloatBytesToTensor / ImageToTensor) when present
    /// at the head of the pre-pipeline. Skipped on JSON-array inputs.
    decoder: Option<WasmTransform>,
    /// Real preprocessors (Normalize, etc.) that operate on wire-format
    /// tensor bytes. Always runs, regardless of input format.
    preprocess: Option<WasmTransform>,
    backend: Box<dyn InferenceBackend>,
    post: Option<WasmTransform>,
    /// Determined from schema.json at load time.
    input_mode: InputMode,
    /// Non-empty only for Named mode.
    specs: Vec<InputSpec>,
}

/// Split a single pre-transform config into (decoder, preprocess).
///
/// If `steps[0]` is a known format adapter, the WASM is instantiated twice
/// from the same bytes - once with just that step (decoder), once with the
/// rest (preprocess). Otherwise the whole config becomes the preprocess and
/// there's no decoder.
///
/// Legacy components (no config bytes) are opaque, so we treat the whole
/// transform as a decoder for backward compat with the original
/// "JSON-bypasses-pre" behavior.
fn split_pre(
    engine: &Engine,
    wasm_bytes: &[u8],
    config: Option<&[u8]>,
) -> Result<(Option<WasmTransform>, Option<WasmTransform>)> {
    let Some(cfg_bytes) = config else {
        let decoder = WasmTransform::new(engine, wasm_bytes, None)?;
        return Ok((Some(decoder), None));
    };

    let parsed: Value = serde_json::from_slice(cfg_bytes)
        .context("failed to parse pre-transform config as JSON")?;
    let Some(steps) = parsed.get("steps").and_then(Value::as_array) else {
        let pp = WasmTransform::new(engine, wasm_bytes, Some(cfg_bytes))?;
        return Ok((None, Some(pp)));
    };

    let first_is_adapter = steps
        .first()
        .and_then(|s| s.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|t| FORMAT_ADAPTERS.contains(&t));

    if !first_is_adapter {
        let pp = WasmTransform::new(engine, wasm_bytes, Some(cfg_bytes))?;
        return Ok((None, Some(pp)));
    }

    let decoder_cfg = json!({ "steps": [&steps[0]] });
    let decoder_bytes = serde_json::to_vec(&decoder_cfg)?;
    let decoder = WasmTransform::new(engine, wasm_bytes, Some(&decoder_bytes))?;

    let preprocess = if steps.len() > 1 {
        let pp_cfg = json!({ "steps": &steps[1..] });
        let pp_bytes = serde_json::to_vec(&pp_cfg)?;
        Some(WasmTransform::new(engine, wasm_bytes, Some(&pp_bytes))?)
    } else {
        None
    };

    Ok((Some(decoder), preprocess))
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

        let (decoder, preprocess, post) = if pre.is_some() || post.is_some() {
            let engine = WasmTransform::build_engine()?;
            let (decoder, preprocess) = if let Some((wasm, cfg)) = pre {
                split_pre(&engine, wasm, cfg).context("failed to load preprocess transform")?
            } else {
                (None, None)
            };
            let post = post
                .map(|(w, c)| WasmTransform::new(&engine, w, c))
                .transpose()
                .context("failed to load postprocess transform")?;
            (decoder, preprocess, post)
        } else {
            (None, None, None)
        };

        let (input_mode, specs) = inputs::parse_schema(schema);
        tracing::info!(mode = ?input_mode, "pipeline input mode");

        Ok(Self {
            decoder,
            preprocess,
            backend,
            post,
            input_mode,
            specs,
        })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        let json_array_path =
            self.input_mode == InputMode::Single && inputs::looks_like_json_array(raw_input);

        // ── Stage 1: produce wire-format bytes for the preprocess stage. ──
        //
        // Single binary:    run the decoder (if any), else input is already wire format.
        // Single JSON array: skip the decoder, since the JSON decoder already
        //                    produced a tensor; re-encode it as wire format.
        // Named:            convert the JSON object to a flat tensor and encode.
        let pre_input: Vec<u8> = if json_array_path {
            let _span = tracing::info_span!("tensor.decode").entered();
            let (shape, data) = inputs::json_array_to_tensor(raw_input)
                .context("failed to decode JSON array input")?;
            tensor::encode(&shape, &data)
        } else if self.input_mode == InputMode::Named {
            let _span = tracing::info_span!("tensor.decode").entered();
            let (shape, data) = inputs::json_to_tensor(raw_input, &self.specs)
                .context("failed to encode JSON input to tensor")?;
            tensor::encode(&shape, &data)
        } else {
            match &mut self.decoder {
                Some(t) => {
                    let _span = tracing::info_span!("wasm.decoder").entered();
                    t.run(raw_input)?
                }
                None => raw_input.to_vec(),
            }
        };

        // ── Stage 2: run preprocess (Normalize, etc.) on wire-format bytes. ──
        let pre_output: Vec<u8> = match &mut self.preprocess {
            Some(t) => {
                let _span = tracing::info_span!("wasm.preprocess").entered();
                t.run(&pre_input)?
            }
            None => pre_input,
        };

        // ── Stage 3: decode wire format and run backend. ──
        let (shape, data) = tensor::decode(&pre_output)?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");
        let (out_shape, out_data) = {
            let _span = tracing::info_span!("backend.infer").entered();
            self.backend.infer(&shape, data)?
        };

        // ── Stage 4: encode output and apply post-transform. ──
        let output_tensor_bytes = {
            let _span = tracing::info_span!("tensor.encode").entered();
            tensor::encode(&out_shape, &out_data)
        };
        match &mut self.post {
            Some(t) => {
                let _span = tracing::info_span!("wasm.postprocess").entered();
                t.run(&output_tensor_bytes)
            }
            None => Ok(output_tensor_bytes),
        }
    }
}
