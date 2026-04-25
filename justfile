data_dir      := "./data"
static_dir    := "./static"
otel_endpoint := "http://localhost:4317"

# ── cluster lifecycle ────────────────────────────────────────────────────────

# Bring up the full edgeflow dev cluster (recreates from scratch)
[group('cluster')]
up: _banner _preflight down build-images cluster-create push-images apply-observability apply-server apply-inference
    @bash scripts/dev/next-steps.sh

# Tear down the edgeflow dev cluster
[group('cluster')]
down:
    bash scripts/dev/cluster-delete.sh

# Create a fresh k3d cluster and label its nodes
[group('cluster')]
cluster-create:
    bash scripts/dev/cluster-create.sh

# ── images ───────────────────────────────────────────────────────────────────

# Pull and tag vendored observability images
pull:
    bash scripts/dev/pull-vendor-images.sh

# Build the server + inference docker images
build-images:
    bash scripts/dev/build-images.sh

# Push vendor + app images to the local registry
push-images:
    bash scripts/dev/push-images.sh

# ── partial reapply (for iteration during a dev session) ─────────────────────

# Re-apply the observability manifest and wait
[group('iteration')]
apply-observability:
    bash scripts/dev/apply-observability.sh

# Re-apply the server manifest and wait
[group('iteration')]
apply-server:
    bash scripts/dev/apply-server.sh

# Re-apply the inference services
[group('iteration')]
apply-inference:
    bash scripts/dev/apply-inference.sh

# Rebuild images and roll the running edgeflow-server pod
[group('iteration')]
deploy-server: build-images push-images
    kubectl rollout restart deployment/edgeflow-server
    kubectl rollout status deployment/edgeflow-server --timeout=120s

# ── host-side build (for hot-reload dev workflows) ───────────────────────────

# Build everything (transforms + UI + native server binary)
build: build-transforms build-ui build-server

# Build the WASM transforms component + the PyO3 extension for the SDK
build-transforms:
    cd crates/transforms && \
        cargo build --target wasm32-wasip2 --release
    cp crates/transforms/target/wasm32-wasip2/release/_lib.wasm \
        apps/sdk/edgeflow/wasm/standard_pipeline.wasm
    cd apps/sdk && uv run maturin develop --features python

# Build the Svelte UI and copy output to static/ (used by dev-server)
build-ui:
    cd apps/ui && npm run build
    rm -rf {{static_dir}}
    cp -r apps/ui/build {{static_dir}}

# Build the native server binary in release mode
build-server:
    cargo build --release -p edgeflow-server

# ── dev hot-reload ───────────────────────────────────────────────────────────

# Run the server natively (with OTEL if the observability stack is up)
dev-server:
    EDGEFLOW_DATA_DIR={{data_dir}} EDGEFLOW_STATIC_DIR={{static_dir}} \
    OTEL_EXPORTER_OTLP_ENDPOINT={{otel_endpoint}} \
    PROMETHEUS_URL=http://localhost:9090 \
    RUST_LOG=edgeflow_server=debug,tower_http=debug \
    cargo run -p edgeflow-server

# Run an inference pod natively against the dev server. Example: just dev-inference iris-inference
dev-inference target:
    EDGEFLOW_SERVER=http://localhost:5000 \
    EDGEFLOW_TARGET={{target}} \
    EDGEFLOW_MQTT_URL=mqtt://localhost:1883 \
    OTEL_EXPORTER_OTLP_ENDPOINT={{otel_endpoint}} \
    RUST_LOG=edgeflow_inference=debug \
    cargo run -p edgeflow-inference

# Run the Svelte dev server
dev-ui:
    cd apps/ui && npm run dev

# ── tests / docs / housekeeping ──────────────────────────────────────────────

# Run a load test against a target. Example: just bench iris-inference 50 120s
bench target users="10" duration="60s":
    cd scripts/test-load && ./bench.sh {{target}} {{users}} {{duration}}

# Run Rust unit tests
test:
    cargo test --workspace

# Run tutorial end-to-end tests (boots docker compose stack, trains, infers)
test-e2e:
    cd tests/e2e && uv run pytest -v

# Run MLflow compatibility tests against a running server
test-compat uri="http://localhost:5000":
    cd scripts && uv run python test/test_mlflow_compat.py --uri {{uri}}

# Generate the iris.onnx fixture used by inference tests and benches
gen-bench-model:
    cd scripts && uv run python test/gen_bench_model.py

# Build and serve the Sphinx documentation
docs:
    mkdir -p docs/book/_static/diagrams
    for f in docs/book/diagrams/*.d2; do d2 "$f" "docs/book/_static/diagrams/$(basename $f .d2).svg"; done
    cd docs/book && uv run sphinx-build -b html . _build/html
    cd docs/book && uv run python -m http.server --bind 127.0.0.1 --directory _build/html

# Clean build artifacts
clean:
    cargo clean
    rm -rf {{static_dir}} apps/ui/build apps/ui/node_modules

# ── private helpers ──────────────────────────────────────────────────────────

[private]
_banner:
    @bash scripts/dev/banner.sh

[private]
_preflight:
    @bash scripts/dev/preflight.sh
