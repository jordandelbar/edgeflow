/// Criterion benchmarks for the inference hot path.
///
/// Groups:
///   tensor/{encode,decode}/<n>  - wire-format codec at three tensor sizes
///   backend/{infer_small,infer_large} - raw ORT session.run() on pre-decoded data
///   pipeline/{infer_small,infer_large} - full path: decode → backend → encode
///   pool_concurrent/<model>/<threads> - N threads, each owning a Pipeline,
///       hammering inference simultaneously. Surfaces any contention regression
///       from sharing a single backend session across pool slots.
///
/// Before running, generate the model fixtures:
///   just gen-bench-model
///
/// Then:
///   cargo bench -p edgeflow-inference
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use edgeflow_inference::{backend, pipeline, tensor};
use std::hint::black_box;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn load_model(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path)
        .unwrap_or_else(|_| panic!("fixture {name} missing - run `just gen-bench-model`"))
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
        let mut encoded = Vec::new();
        tensor::encode_into(&shape, &data, &mut encoded);

        g.throughput(Throughput::Bytes((n * 4) as u64));

        // Mirrors production: encode_into against a hoisted, reused buffer.
        g.bench_with_input(
            BenchmarkId::new("encode_into", n),
            &(shape, &data),
            |b, (shape, data)| {
                let mut buf = Vec::new();
                b.iter(|| tensor::encode_into(black_box(shape), black_box(data), &mut buf))
            },
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

    // Small - iris model, [1, 4]
    {
        let model = load_model("iris.onnx");
        let mut b = backend::build_backend();
        b.load(&model).expect("failed to load iris model");
        let shape = [1usize, 4];
        let data = vec![5.1f32, 3.5, 1.4, 0.2];
        let mut out = Vec::new();
        g.bench_function("infer_small", |bench| {
            bench.iter(|| {
                b.infer(black_box(&shape), black_box(&data), &mut out)
                    .unwrap()
            })
        });
    }

    // Large - large model, [1, 4096]
    {
        let model = load_model("large.onnx");
        let mut b = backend::build_backend();
        b.load(&model).expect("failed to load large model");
        let shape = [1usize, 4096];
        let data: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
        let mut out = Vec::new();
        g.bench_function("infer_large", |bench| {
            bench.iter(|| {
                b.infer(black_box(&shape), black_box(&data), &mut out)
                    .unwrap()
            })
        });
    }

    g.finish();
}

// ── full pipeline: decode → backend → encode ─────────────────────────────────

fn bench_pipeline(c: &mut Criterion) {
    let mut g = c.benchmark_group("pipeline");

    // Small - iris model, [1, 4] → [1, 3]
    {
        let model = load_model("iris.onnx");
        let mut p = pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None)
            .expect("failed to build pipeline (iris)");
        let mut input = Vec::new();
        tensor::encode_into(&[1, 4], &[5.1f32, 3.5, 1.4, 0.2], &mut input);
        g.bench_function("infer_small", |b| {
            b.iter(|| p.infer(black_box(&input)).unwrap())
        });
    }

    // Large - large model, [1, 4096] → [1, 10]
    {
        let model = load_model("large.onnx");
        let mut p = pipeline::Pipeline::new(backend::build_backend(), &model, None, None, None)
            .expect("failed to build pipeline (large)");
        let data: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
        let mut input = Vec::new();
        tensor::encode_into(&[1, 4096], &data, &mut input);
        g.bench_function("infer_large", |b| {
            b.iter(|| p.infer(black_box(&input)).unwrap())
        });
    }

    g.finish();
}

// ── pool concurrency: N threads, each owning a Pipeline ─────────────────────
//
// Reported time is wall-clock per single inference, amortized across threads.
// To compare with the single-pipeline `bench_pipeline` numbers: if 4 threads
// finish 1000 infers in 100 ms wall, criterion reports 100 µs/iter (and the
// aggregate throughput is `n_threads / per_iter`).
//
// Persistent worker threads are spawned once per `bench_function` invocation
// and reused across all of criterion's measurement iterations - the load cost
// of building a Pipeline (model parsing, ORT session creation) does not show
// up in the reported time.

const POOL_THREAD_COUNTS: &[usize] = &[1, 2, 4];

fn bench_pool_concurrent(c: &mut Criterion) {
    let mut g = c.benchmark_group("pool_concurrent");
    g.sample_size(20);

    for (model_name, file, shape, data) in [
        (
            "iris",
            "iris.onnx",
            vec![1usize, 4],
            vec![5.1f32, 3.5, 1.4, 0.2],
        ),
        ("large", "large.onnx", vec![1usize, 4096], {
            let v: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
            v
        }),
    ] {
        let model = load_model(file);
        let mut input = Vec::new();
        tensor::encode_into(&shape, &data, &mut input);
        let input = Arc::new(input);

        for &n_threads in POOL_THREAD_COUNTS {
            g.throughput(Throughput::Elements(n_threads as u64));
            g.bench_function(BenchmarkId::new(model_name, n_threads), |b| {
                let (work_txs, done_rxs, handles) = spawn_workers(n_threads, &model, &input);

                b.iter_custom(|iters| {
                    // Distribute iters across threads. Each worker does
                    // `per_thread` infers, all firing simultaneously.
                    let per_thread = (iters / n_threads as u64).max(1);
                    let start = Instant::now();
                    for tx in &work_txs {
                        tx.send(per_thread).unwrap();
                    }
                    for rx in &done_rxs {
                        rx.recv().unwrap();
                    }
                    start.elapsed()
                });

                // Closing the channels lets workers fall out of their loop.
                drop(work_txs);
                drop(done_rxs);
                for h in handles {
                    h.join().unwrap();
                }
            });
        }
    }

    g.finish();
}

#[allow(clippy::type_complexity)]
fn spawn_workers(
    n: usize,
    model: &[u8],
    input: &Arc<Vec<u8>>,
) -> (
    Vec<mpsc::Sender<u64>>,
    Vec<mpsc::Receiver<()>>,
    Vec<thread::JoinHandle<()>>,
) {
    let mut work_txs = Vec::with_capacity(n);
    let mut done_rxs = Vec::with_capacity(n);
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        let mut p = pipeline::Pipeline::new(backend::build_backend(), model, None, None, None)
            .expect("failed to build pipeline");
        let (work_tx, work_rx) = mpsc::channel::<u64>();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let input = Arc::clone(input);
        let h = thread::spawn(move || {
            while let Ok(count) = work_rx.recv() {
                for _ in 0..count {
                    black_box(p.infer(&input).expect("infer failed"));
                }
                if done_tx.send(()).is_err() {
                    break;
                }
            }
        });
        work_txs.push(work_tx);
        done_rxs.push(done_rx);
        handles.push(h);
    }
    (work_txs, done_rxs, handles)
}

criterion_group!(
    benches,
    bench_tensor,
    bench_backend,
    bench_pipeline,
    bench_pool_concurrent
);
criterion_main!(benches);
