# Edgeflow

Train in Python. Serve in Rust.

Edgeflow is an MLflow-compatible experiment tracker, model registry, and
inference server, built for people who can't afford the memory tax of a
Python serving stack. Models run as ONNX (ort or tract), pre/post
processing runs as WASM, and deployments hot-swap without downtime.

## Quickstart

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
