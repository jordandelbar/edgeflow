/// Integration tests for the inference pipeline.
///
/// Tests that don't touch the model fixture (tensor, inputs) always pass.
/// Tests that require the model fixture need it generated first:
///   just gen-bench-model
///
/// Model shape: f32 [N, 4] → f32 [N, 3]  (Gemm + Softmax, no WASM needed)
use std::sync::Arc;

use edgeflow_inference::backend::InferenceBackend;
use edgeflow_inference::{backend, inputs, pipeline, tensor};

fn load_model() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/iris.onnx");
    std::fs::read(path).expect("model fixture missing - run `just gen-bench-model` first")
}

fn loaded_backend(model: &[u8]) -> Arc<dyn InferenceBackend> {
    let mut b = backend::build_backend();
    b.load(model).expect("failed to load model");
    Arc::from(b)
}

// ── tensor wire format ───────────────────────────────────────────────────────

#[test]
fn tensor_encode_decode_roundtrip() {
    let shape = [1usize, 4];
    let data = [5.1f32, 3.5, 1.4, 0.2];
    let mut encoded = Vec::new();
    tensor::encode_into(&shape, &data, &mut encoded);
    let (s, d) = tensor::decode(&encoded).unwrap();
    assert_eq!(s, shape);
    for (a, b) in d.iter().zip(&data) {
        assert!((a - b).abs() < 1e-6, "mismatch: {a} vs {b}");
    }
}

#[test]
fn tensor_decode_rejects_empty() {
    assert!(tensor::decode(&[]).is_err());
}

#[test]
fn tensor_decode_rejects_unknown_dtype() {
    // ndim=1, shape=[4], dtype=99
    let buf = [1u8, 4, 0, 0, 0, 99u8];
    assert!(tensor::decode(&buf).is_err());
}

#[test]
fn tensor_encode_header_layout() {
    // Verify the exact wire layout: ndim(1) | dtype(1) | pad(2) | shape dims(4 each) | data
    let mut encoded = Vec::new();
    tensor::encode_into(&[2usize, 3], &[0f32; 6], &mut encoded);
    assert_eq!(encoded[0], 2); // ndim
    assert_eq!(encoded[1], 1); // dtype = f32
    assert_eq!(encoded[2], 0); // padding
    assert_eq!(encoded[3], 0); // padding
    assert_eq!(u32::from_le_bytes(encoded[4..8].try_into().unwrap()), 2); // dim 0
    assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 3); // dim 1
                                                                           // data starts at byte 12 (= 4 + 2*4), always 4-byte aligned
    assert_eq!(encoded.len(), 4 + 2 * 4 + 6 * 4); // total size
}

// ── input parsing (Named mode, no model needed) ──────────────────────────────

#[test]
fn inputs_single_mode_when_no_schema() {
    let (mode, specs) = inputs::parse_schema(None);
    assert_eq!(mode, inputs::InputMode::Single);
    assert!(specs.is_empty());
}

#[test]
fn inputs_single_mode_on_empty_schema() {
    let (mode, _) = inputs::parse_schema(Some(b""));
    assert_eq!(mode, inputs::InputMode::Single);
}

#[test]
fn inputs_named_mode_parses_fields() {
    let schema = br#"{"input":{"format":"json","fields":[
        {"name":"age","type":"float"},
        {"name":"income","type":"float"}
    ]}}"#;
    let (mode, specs) = inputs::parse_schema(Some(schema));
    assert_eq!(mode, inputs::InputMode::Named);
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].name, "age");
    assert_eq!(specs[1].name, "income");
}

#[test]
fn inputs_json_passthrough_fields() {
    let schema = br#"{"input":{"format":"json","fields":[
        {"name":"a","type":"float"},
        {"name":"b","type":"float"}
    ]}}"#;
    let (_, specs) = inputs::parse_schema(Some(schema));
    let body = br#"{"a": 1.5, "b": 2.5}"#;
    let (shape, data) = inputs::json_to_tensor(body, &specs).unwrap();
    assert_eq!(shape, vec![1, 2]);
    assert!((data[0] - 1.5).abs() < 1e-6);
    assert!((data[1] - 2.5).abs() < 1e-6);
}

#[test]
fn inputs_json_ordinal_encoding() {
    use inputs::{Encoding, InputSpec};
    use std::collections::HashMap;

    let map: HashMap<String, f32> = [("cat".into(), 0.0), ("dog".into(), 1.0)].into();
    let specs = vec![InputSpec {
        name: "animal".into(),
        encoding: Some(Encoding::Ordinal(map)),
    }];
    let (_, data) = inputs::json_to_tensor(br#"{"animal":"dog"}"#, &specs).unwrap();
    assert!((data[0] - 1.0).abs() < 1e-6);
}

#[test]
fn inputs_json_one_hot_encoding() {
    use inputs::{Encoding, InputSpec};

    let specs = vec![InputSpec {
        name: "color".into(),
        encoding: Some(Encoding::OneHot {
            categories: vec!["red".into(), "green".into(), "blue".into()],
        }),
    }];
    let (_, data) = inputs::json_to_tensor(br#"{"color":"green"}"#, &specs).unwrap();
    assert_eq!(data, vec![0.0f32, 1.0, 0.0]);
}

#[test]
fn inputs_json_missing_field_errors() {
    use inputs::InputSpec;

    let specs = vec![InputSpec {
        name: "x".into(),
        encoding: None,
    }];
    assert!(inputs::json_to_tensor(br#"{"y": 1.0}"#, &specs).is_err());
}

// ── plain JSON array input (Single-mode body sniffing) ───────────────────────

#[test]
fn inputs_looks_like_json_array_detects_bracket() {
    assert!(inputs::looks_like_json_array(b"[1,2,3]"));
    assert!(inputs::looks_like_json_array(b"  \t\n[1,2,3]"));
    assert!(inputs::looks_like_json_array(b"[[1,2],[3,4]]"));
}

#[test]
fn inputs_looks_like_json_array_rejects_other_bodies() {
    assert!(!inputs::looks_like_json_array(b""));
    assert!(!inputs::looks_like_json_array(b"   "));
    assert!(!inputs::looks_like_json_array(b"{\"a\":1}"));
    // Binary tensor header: ndim=1, dtype=1 - first byte is 0x01, not '['.
    assert!(!inputs::looks_like_json_array(&[1u8, 1, 0, 0, 4, 0, 0, 0]));
}

#[test]
fn inputs_json_array_1d_auto_batches() {
    let (shape, data) = inputs::json_array_to_tensor(b"[5.1, 3.5, 1.4, 0.2]").unwrap();
    assert_eq!(shape, vec![1, 4]);
    assert_eq!(data, vec![5.1f32, 3.5, 1.4, 0.2]);
}

#[test]
fn inputs_json_array_2d_kept_as_is() {
    let (shape, data) = inputs::json_array_to_tensor(b"[[1.0, 2.0], [3.0, 4.0]]").unwrap();
    assert_eq!(shape, vec![2, 2]);
    assert_eq!(data, vec![1.0f32, 2.0, 3.0, 4.0]);
}

#[test]
fn inputs_json_array_explicit_batch_of_one() {
    let (shape, data) = inputs::json_array_to_tensor(b"[[5.1, 3.5, 1.4, 0.2]]").unwrap();
    assert_eq!(shape, vec![1, 4]);
    assert_eq!(data, vec![5.1f32, 3.5, 1.4, 0.2]);
}

#[test]
fn inputs_json_array_accepts_integers() {
    let (shape, data) = inputs::json_array_to_tensor(b"[1, 2, 3]").unwrap();
    assert_eq!(shape, vec![1, 3]);
    assert_eq!(data, vec![1.0f32, 2.0, 3.0]);
}

#[test]
fn inputs_json_array_3d_nested() {
    let (shape, data) = inputs::json_array_to_tensor(b"[[[1,2],[3,4]],[[5,6],[7,8]]]").unwrap();
    assert_eq!(shape, vec![2, 2, 2]);
    assert_eq!(data, vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
}

#[test]
fn inputs_json_array_rejects_ragged() {
    assert!(inputs::json_array_to_tensor(b"[[1, 2], [3, 4, 5]]").is_err());
}

#[test]
fn inputs_json_array_rejects_non_numeric() {
    assert!(inputs::json_array_to_tensor(br#"["a", "b"]"#).is_err());
}

#[test]
fn inputs_json_array_rejects_object_body() {
    assert!(inputs::json_array_to_tensor(br#"{"a": 1}"#).is_err());
}

#[test]
fn inputs_json_array_rejects_empty_outer() {
    assert!(inputs::json_array_to_tensor(b"[]").is_err());
}

// ── backend + pipeline (model fixture required) ──────────────────────────────

#[test]
fn backend_loads_and_infers() {
    let model = load_model();
    let mut b = backend::build_backend();
    b.load(&model).unwrap();
    let mut buf = Vec::new();
    b.infer(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2], &mut buf)
        .unwrap();
    let (out_shape, out_data) = tensor::decode(&buf).unwrap();
    assert_eq!(out_shape, vec![1, 3], "expected [1, 3] output shape");
    assert_eq!(out_data.len(), 3);
}

#[test]
fn backend_output_sums_to_one() {
    let model = load_model();
    let mut b = backend::build_backend();
    b.load(&model).unwrap();
    let mut buf = Vec::new();
    b.infer(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2], &mut buf)
        .unwrap();
    let (_, out) = tensor::decode(&buf).unwrap();
    let sum: f32 = out.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "softmax should sum to 1, got {sum}"
    );
}

#[test]
fn pipeline_single_mode_end_to_end() {
    let model = load_model();
    let mut p = pipeline::Pipeline::new(loaded_backend(&model), None, None, None).unwrap();
    let mut input = Vec::new();
    tensor::encode_into(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2], &mut input);
    let output = p.infer(&input).unwrap();
    let (shape, data) = tensor::decode(&output).unwrap();
    assert_eq!(shape, vec![1, 3]);
    assert_eq!(data.len(), 3);
    let sum: f32 = data.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5);
}

#[test]
fn pipeline_rejects_wrong_input_size() {
    let model = load_model();
    let mut p = pipeline::Pipeline::new(loaded_backend(&model), None, None, None).unwrap();
    // Send only 2 features instead of 4 - shape mismatch should error
    let mut input = Vec::new();
    tensor::encode_into(&[1, 2], &[5.1f32, 3.5], &mut input);
    assert!(p.infer(&input).is_err());
}

#[test]
fn pipeline_single_mode_accepts_json_array() {
    let model = load_model();
    let mut p = pipeline::Pipeline::new(loaded_backend(&model), None, None, None).unwrap();
    let output = p.infer(b"[5.1, 3.5, 1.4, 0.2]").unwrap();
    let (shape, data) = tensor::decode(&output).unwrap();
    assert_eq!(shape, vec![1, 3]);
    let sum: f32 = data.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "softmax should sum to 1");
}

/// JSON arrays must skip the format-adapter at the head of the pre-pipeline
/// (FloatBytesToTensor / ImageToTensor) - it's a bytes-to-tensor conversion
/// the JSON decoder already did - while still running every preprocessor that
/// follows (Normalize, etc.). Both wire formats must produce the same
/// prediction for the same logical input.
#[test]
fn pipeline_json_array_runs_preprocess_after_decoder() {
    let model = load_model();
    let wasm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sdk/edgeflow/wasm/standard_pipeline.wasm"
    ))
    .expect("standard_pipeline.wasm missing - run `just build-transforms`");

    // Two-step pipeline: format adapter + real preprocessor. Without the fix,
    // JSON inputs skip both, so Normalize never runs and predictions diverge.
    let pre_config = br#"{"steps":[
        {"type":"float_to_tensor","n_features":4},
        {"type":"normalize","mean":[5.84,3.05,3.74,1.20],"std":[0.83,0.43,1.77,0.76]}
    ]}"#;

    let mut p = pipeline::Pipeline::new(
        loaded_backend(&model),
        Some((&wasm, pre_config.as_slice())),
        None,
        None,
    )
    .unwrap();

    let from_json = p.infer(b"[5.1, 3.5, 1.4, 0.2]").unwrap();
    let raw_floats: Vec<u8> = [5.1f32, 3.5, 1.4, 0.2]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let from_binary = p.infer(&raw_floats).unwrap();

    let (s_json, d_json) = tensor::decode(&from_json).unwrap();
    let (s_bin, d_bin) = tensor::decode(&from_binary).unwrap();
    assert_eq!(s_json, s_bin);
    for (a, b) in d_json.iter().zip(d_bin) {
        assert!(
            (a - b).abs() < 1e-6,
            "JSON path diverged from binary path (preprocess skipped?): {a} vs {b}",
        );
    }
}

/// Regression: the JSON-array dispatch must sniff `raw_input`, not the
/// post-pre-transform body. Real iris deployments inject a `FloatBytesToTensor`
/// pre-transform that would mangle a JSON body if the pipeline let it run.
#[test]
fn pipeline_json_array_skips_format_adapter() {
    let model = load_model();
    let wasm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sdk/edgeflow/wasm/standard_pipeline.wasm"
    ))
    .expect("standard_pipeline.wasm missing - run `just build-transforms`");
    let pre_config = br#"{"steps":[{"type":"float_to_tensor","n_features":4}]}"#;

    let mut p = pipeline::Pipeline::new(
        loaded_backend(&model),
        Some((&wasm, pre_config.as_slice())),
        None,
        None,
    )
    .unwrap();

    // Run the same logical input two ways and assert the outputs match.
    // If the pre-transform ran on the JSON body, it would interpret the ASCII
    // bytes as f32 LE garbage and produce a different (or failing) prediction.
    let from_json = p.infer(b"[5.1, 3.5, 1.4, 0.2]").unwrap();
    let raw_floats: Vec<u8> = [5.1f32, 3.5, 1.4, 0.2]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let from_binary = p.infer(&raw_floats).unwrap();

    let (s_json, d_json) = tensor::decode(&from_json).unwrap();
    let (s_bin, d_bin) = tensor::decode(&from_binary).unwrap();
    assert_eq!(s_json, s_bin);
    for (a, b) in d_json.iter().zip(d_bin) {
        assert!(
            (a - b).abs() < 1e-6,
            "JSON path diverged from binary path: {a} vs {b}",
        );
    }
}
