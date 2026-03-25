#!/usr/bin/env bash
# End-to-end PoC setup: build → k3d → deploy → train → test
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ── build images ──────────────────────────────────────────────────────────────
echo "==> building edgeflow-server image..."
docker build -f deploy/Dockerfile.server -t edgeflow-server:dev .

echo "==> building edgeflow-inference image..."
docker build -f deploy/Dockerfile.inference -t edgeflow-inference:dev .

# ── k3d cluster ───────────────────────────────────────────────────────────────
echo "==> creating k3d cluster..."
if k3d cluster list | grep -q "^edgeflow"; then
    echo "    cluster already exists, deleting..."
    k3d cluster delete edgeflow
fi
k3d cluster create --config deploy/k3d-cluster.yaml

echo "==> importing images into cluster..."
k3d image import edgeflow-server:dev edgeflow-inference:dev -c edgeflow

# ── deploy ────────────────────────────────────────────────────────────────────
echo "==> deploying manifests..."
kubectl apply -f deploy/manifests/

echo "==> waiting for edgeflow-server to be ready..."
kubectl rollout status deployment/edgeflow-server --timeout=120s

# ── train & push ──────────────────────────────────────────────────────────────
# Server is reachable on localhost:5000 via k3d port mapping.
# The inference pod is already running and polling for a deployment.
echo "==> running training script..."
EDGEFLOW_SERVER=http://localhost:5000 \
EDGEFLOW_TARGET=iris-inference \
    uv run --project scripts python scripts/train_iris.py

# ── wait for inference ────────────────────────────────────────────────────────
# Inference pod fetches the deployment, downloads artifacts, loads pipeline,
# then /health starts responding → readiness probe passes.
echo "==> waiting for edgeflow-inference to be ready..."
kubectl rollout status deployment/edgeflow-inference --timeout=300s

# ── smoke test ────────────────────────────────────────────────────────────────
echo ""
echo "==> smoke test (setosa sample: sepal=5.1x3.5 petal=1.4x0.2)..."
python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \
    | curl -s -X POST http://localhost:8080/infer --data-binary @- \
    | python3 -m json.tool

echo ""
echo "done. cluster is up at localhost:5000 (server) and localhost:8080 (inference)."
echo "to tear down: k3d cluster delete edgeflow"
