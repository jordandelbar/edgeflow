# Edgeflow

Train in Python. Serve in Rust.

[![CI](https://github.com/jordandelbar/edgeflow/actions/workflows/ci.yml/badge.svg)](https://github.com/jordandelbar/edgeflow/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-orange.svg)](Cargo.toml)

Edgeflow is an MLflow-compatible experiment tracker, model registry, and
inference server, built for people who can't afford the memory tax of a
Python serving stack. Models run as ONNX (ort or tract), pre/post
processing runs as WASM, and deployments hot-swap without downtime.

![Edgeflow deployment detail: live throughput, p50/p95/p99 latency, per-pod memory and health](docs/screenshots/deployment-detail.png)

## What you get

- MLflow-compatible tracking and model registry
- ONNX inference (ort or tract backend)
- Hot-swap deploys with no downtime
- WASM pre/post processing
- Runs on Kubernetes (multi target deployments) or plain docker-compose (single target deployment)
- OpenTelemetry metrics and traces out of the box

## Demo Quickstart

Bring up a local server and one inference pod:

```bash
docker compose -f deploy/docker-compose.yaml up --build
```

The first build compiles everything from source and takes a few minutes.
Once it's running, train and deploy a model:

```python
import edgeflow

with edgeflow.start_run() as run:
    edgeflow.log_model(model, "iris")

mv = edgeflow.register(run.info.run_id, name="iris")
edgeflow.deploy(name="iris", version=mv.version, target="my-target")
```

Then call it:

```bash
curl -X POST http://localhost:5000/api/v1/targets/my-target/infer \
     -d '[5.1, 3.5, 1.4, 0.2]'
```

The compose path runs a single inference pod. Any target name works,
but only the latest deployment is active at a time. Multi-target
deployments, rolling upgrades, and resource patching require the
Kubernetes path.
