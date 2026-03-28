data_dir := "./data"
static_dir := "./static"

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

# Build the server image, import into k3d, and rollout restart
deploy-server:
    docker build -f deploy/server.Dockerfile -t edgeflow-server:dev .
    k3d image import edgeflow-server:dev -c edgeflow
    kubectl rollout restart deployment/edgeflow-server
    kubectl rollout status deployment/edgeflow-server --timeout=120s

# Run the server in dev mode
dev-server:
    EDGEFLOW_DATA_DIR={{data_dir}} EDGEFLOW_STATIC_DIR={{static_dir}} \
    RUST_LOG=edgeflow_server=debug,tower_http=debug \
    cargo run -p edgeflow-server

# Run the Svelte dev server
dev-ui:
    cd apps/ui && npm run dev

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
