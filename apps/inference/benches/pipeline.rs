/// Criterion benchmarks for the inference hot path.
///
/// Groups:
///   tensor/{encode,decode}/<n>  — wire-format codec at three tensor sizes
///   backend/{infer_small,infer_large} — raw ORT session.run() on pre-decoded data
///   pipeline/{infer_small,infer_large} — full path: decode → backend → encode
///
/// Before running, generate the model fixtures:
///   uv run python scripts/gen_bench_model.py
///
/// Then:
///   cargo bench -p edgeflow-inference
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use edgeflow_inference::{backend, pipeline, tensor};

fn load_model(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("fixture {name} missing — run `uv run python scripts/gen_bench_model.py`")
    })
}

// ── tensor codec at three representative sizes ────────────────────────────────
//   4       =  16 B  (iris / tiny tabular)
//   4_096   =  16 KB (wide feature vector / embeddings)
//   65_536  = 256 KB (image-scale, e.g. 256×256 greyscale)

const TENSOR_SIZES: &[usize] = &[4, 4_096, 65_536];

fn bench_tensor(c: &mut Criterion) {
    let mut g = c.benchmark_group("tensor");

    for &n in TENSOR_SIZES {
        let data: Vec<f32> = (0..n).map(|i| i as f32 * 0.001).collect();
        let shape = [1usize, n];
        let encoded = tensor::encode(&shape, &data);

        g.throughput(Throughput::Bytes((n * 4) as u64));

        g.bench_with_input(
            BenchmarkId::new("encode", n),
            &(shape, &data),
            |b, (shape, data)| b.iter(|| tensor::encode(black_box(shape), black_box(data))),
        );
        g.bench_with_input(BenchmarkId::new("decode", n), &encoded, |b, encoded| {
            b.iter(|| tensor::decode(black_box(encoded)).unwrap())
        });
    }

    g.finish();
}

// ── ORT backend: raw session.run() without encode/decode overhead ─────────────

fn bench_backend(c: &mut Criterion) {
    let mut g = c.benchmark_group("backend");

    // Small — iris model, [1, 4]
    {
        let model = load_model("iris.onnx");
        let mut b = backend::build_backend();
        b.load(&model).expect("failed to load iris model");
        let shape = [1usize, 4];
        let data = vec![5.1f32, 3.5, 1.4, 0.2];
        g.bench_function("infer_small", |bench| {
            bench.iter(|| b.infer(black_box(&shape), black_box(&data)).unwrap())
        });
    }

    // Large — large model, [1, 4096]
    {
        let model = load_model("large.onnx");
        let mut b = backend::build_backend();
        b.load(&model).expect("failed to load large model");
        let shape = [1usize, 4096];
        let data: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
        g.bench_function("infer_large", |bench| {
            bench.iter(|| b.infer(black_box(&shape), black_box(&data)).unwrap())
        });
    }

    g.finish();
}

// ── full pipeline: decode → backend → encode ─────────────────────────────────

fn bench_pipeline(c: &mut Criterion) {
    let mut g = c.benchmark_group("pipeline");

    // Small — iris model, [1, 4] → [1, 3]
    {
        let model = load_model("iris.onnx");
        let mut p = pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None)
            .expect("failed to build pipeline (iris)");
        let input = tensor::encode(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2]);
        g.bench_function("infer_small", |b| {
            b.iter(|| p.infer(black_box(&input)).unwrap())
        });
    }

    // Large — large model, [1, 4096] → [1, 10]
    {
        let model = load_model("large.onnx");
        let mut p = pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None)
            .expect("failed to build pipeline (large)");
        let data: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
        let input = tensor::encode(&[1, 4096], &data);
        g.bench_function("infer_large", |b| {
            b.iter(|| p.infer(black_box(&input)).unwrap())
        });
    }

    g.finish();
}

criterion_group!(benches, bench_tensor, bench_backend, bench_pipeline);
criterion_main!(benches);
