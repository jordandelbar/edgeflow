data_dir := "./data"
static_dir := "./static"

# Build everything
build: build-transforms build-ui build-server

# Compile the standard Rust transforms:
#   - WASM component → python/edgeflow/wasm/standard_pipeline.wasm  (server, ~150 KB)
#   - Native PyO3 extension → edgeflow/_lib.so                      (local execution)
build-transforms:
    cd crates/edgeflow-transforms && \
        cargo build --target wasm32-wasip2 --release
    cp crates/edgeflow-transforms/target/wasm32-wasip2/release/_lib.wasm \
        python/edgeflow/wasm/standard_pipeline.wasm
    cd python && uv run maturin develop --features python

# Build the Svelte UI and copy output to static/
build-ui:
    cd ui && npm install && npm run build
    rm -rf {{static_dir}}
    cp -r ui/build {{static_dir}}

# Build the server in release mode
build-server:
    cargo build --release -p edgeflow-server

# Run the server in dev mode
dev-server:
    EDGEFLOW_DATA_DIR={{data_dir}} EDGEFLOW_STATIC_DIR={{static_dir}} \
    RUST_LOG=edgeflow_server=debug,tower_http=debug \
    cargo run -p edgeflow-server

# Run the Svelte dev server
dev-ui:
    cd ui && npm run dev

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
    rm -rf {{static_dir}} ui/build ui/node_modules
