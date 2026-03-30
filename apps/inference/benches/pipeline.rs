/// Criterion benchmarks for the inference hot path.
///
/// Covers each layer individually so you can see exactly where time goes:
///   tensor/encode  — wire-format serialisation
///   tensor/decode  — wire-format deserialisation
///   backend/infer  — raw ORT session.run() on pre-decoded data
///   pipeline/infer — full path: decode → backend → encode (no WASM)
///
/// Before running, generate the model fixture:
///   python scripts/gen_bench_model.py
///
/// Then:
///   cargo bench -p edgeflow-inference
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use edgeflow_inference::{backend, pipeline, tensor};

fn load_model() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/iris.onnx");
    std::fs::read(path)
        .expect("model fixture missing — run `python scripts/gen_bench_model.py` first")
}

// Representative iris sample: sepal_len, sepal_w, petal_len, petal_w
const SHAPE: [usize; 2] = [1, 4];
const INPUT: [f32; 4] = [5.1, 3.5, 1.4, 0.2];

fn bench_tensor(c: &mut Criterion) {
    let mut g = c.benchmark_group("tensor");
    let encoded = tensor::encode(&SHAPE, &INPUT);

    g.bench_function("encode", |b| {
        b.iter(|| tensor::encode(black_box(&SHAPE), black_box(&INPUT)))
    });

    g.bench_function("decode", |b| {
        b.iter(|| tensor::decode(black_box(&encoded)).unwrap())
    });

    g.finish();
}

fn bench_backend(c: &mut Criterion) {
    let mut g = c.benchmark_group("backend");
    let model = load_model();

    let mut b = backend::build_backend();
    b.load(&model)
        .expect("failed to load model — run scripts/gen_bench_model.py first");

    g.bench_function("infer", |b_bench| {
        b_bench.iter(|| b.infer(black_box(&SHAPE), black_box(&INPUT)).unwrap())
    });

    g.finish();
}

fn bench_pipeline(c: &mut Criterion) {
    let mut g = c.benchmark_group("pipeline");
    let model = load_model();

    let mut p = pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None)
        .expect("failed to build pipeline — run scripts/gen_bench_model.py first");

    let input_bytes = tensor::encode(&SHAPE, &INPUT);

    g.bench_function("infer", |b| {
        b.iter(|| p.infer(black_box(&input_bytes)).unwrap())
    });

    g.finish();
}

criterion_group!(benches, bench_tensor, bench_backend, bench_pipeline);
criterion_main!(benches);
