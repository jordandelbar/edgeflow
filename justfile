data_dir     := "./data"
static_dir   := "./static"
otel_endpoint := "http://localhost:4317"

# Build everything
build: build-transforms build-ui build-server

# Compile the standard Rust transforms:
#   - WASM component → apps/sdk/edgeflow/wasm/standard_pipeline.wasm  (server, ~150 KB)
#   - Native PyO3 extension → edgeflow/_lib.so                        (local execution)
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

# Build the server in release mode
build-server:
    cargo build --release -p edgeflow-server

# Apply the observability stack (OTel Collector, Prometheus, Tempo, Grafana)
deploy-observability:
    kubectl apply -f deploy/manifests/observability.yaml
    kubectl rollout status deployment/otelcol --timeout=120s
    kubectl rollout status deployment/prometheus --timeout=120s
    kubectl rollout status deployment/tempo --timeout=120s
    kubectl rollout status deployment/grafana --timeout=120s

# Build the server image, import into k3d, and rollout restart
deploy-server:
    docker build -f deploy/server.Dockerfile -t edgeflow-server:dev .
    k3d image import edgeflow-server:dev -c edgeflow
    kubectl rollout restart deployment/edgeflow-server
    kubectl rollout status deployment/edgeflow-server --timeout=120s

# Run the server in dev mode (with OTEL if the observability stack is up)
dev-server:
    EDGEFLOW_DATA_DIR={{data_dir}} EDGEFLOW_STATIC_DIR={{static_dir}} \
    OTEL_EXPORTER_OTLP_ENDPOINT={{otel_endpoint}} \
    PROMETHEUS_URL=http://localhost:9090 \
    RUST_LOG=edgeflow_server=debug,tower_http=debug \
    cargo run -p edgeflow-server

# Run an inference pod locally against the dev server (requires EDGEFLOW_TARGET)
# Example: just dev-inference iris-inference
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

# Run a load test against a target
# Example: just bench iris-inference
# Example: just bench adult-inference 50 120s
bench target users="10" duration="60s":
    cd scripts/test-load && ./bench.sh {{target}} {{users}} {{duration}}

# Run Rust unit tests
test:
    cargo test

# Run MLflow compatibility tests against a running server
test-compat uri="http://localhost:5000":
    python scripts/test_mlflow_compat.py --uri {{uri}}

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
