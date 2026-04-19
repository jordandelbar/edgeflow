/// Integration tests for the inference pipeline.
///
/// Tests that don't touch the model fixture (tensor, inputs) always pass.
/// Tests that require the model fixture need it generated first:
///   python scripts/gen_bench_model.py
///
/// Model shape: f32 [N, 4] → f32 [N, 3]  (Gemm + Softmax, no WASM needed)
use edgeflow_inference::{backend, inputs, pipeline, tensor};

fn load_model() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/iris.onnx");
    std::fs::read(path)
        .expect("model fixture missing - run `python scripts/gen_bench_model.py` first")
}

// ── tensor wire format ───────────────────────────────────────────────────────

#[test]
fn tensor_encode_decode_roundtrip() {
    let shape = [1usize, 4];
    let data = [5.1f32, 3.5, 1.4, 0.2];
    let encoded = tensor::encode(&shape, &data);
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
    let encoded = tensor::encode(&[2usize, 3], &[0f32; 6]);
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

// ── backend + pipeline (model fixture required) ──────────────────────────────

#[test]
fn backend_loads_and_infers() {
    let model = load_model();
    let mut b = backend::build_backend();
    b.load(&model).unwrap();
    let (out_shape, out_data) = b.infer(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2]).unwrap();
    assert_eq!(out_shape, vec![1, 3], "expected [1, 3] output shape");
    assert_eq!(out_data.len(), 3);
}

#[test]
fn backend_output_sums_to_one() {
    let model = load_model();
    let mut b = backend::build_backend();
    b.load(&model).unwrap();
    let (_, out) = b.infer(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2]).unwrap();
    let sum: f32 = out.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "softmax should sum to 1, got {sum}"
    );
}

#[test]
fn pipeline_single_mode_end_to_end() {
    let model = load_model();
    let mut p =
        pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None).unwrap();
    let input = tensor::encode(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2]);
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
    let mut p =
        pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None).unwrap();
    // Send only 2 features instead of 4 - shape mismatch should error
    let input = tensor::encode(&[1, 2], &[5.1f32, 3.5]);
    assert!(p.infer(&input).is_err());
}
