#!/usr/bin/env bash
# End-to-end dev setup: build → k3d → deploy → train → test
#
# New lifecycle (hot-swap):
#   1. Server + inference pods start in parallel.
#   2. Inference pod registers its address with the server (retries until server is up).
#   3. Training creates a deployment record → server calls POST /upgrade on the pod.
#   4. Pod downloads artifacts, swaps pipeline atomically, confirms HEALTHY.
#   5. k8s readiness probe (/health) passes → rollout completes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ── build images ──────────────────────────────────────────────────────────────
echo "==> building images (server + inference-ort)..."
docker buildx bake -f deploy/docker-bake.hcl server inference-ort

# ── k3d cluster ───────────────────────────────────────────────────────────────
echo "==> creating k3d cluster..."
if k3d cluster list | grep -q "^edgeflow"; then
    echo "    cluster already exists, deleting..."
    k3d cluster delete edgeflow
fi
k3d cluster create --config deploy/k3d-cluster.yaml

echo "==> labelling nodes..."
kubectl label node k3d-edgeflow-server-0 edgeflow-role=server --overwrite
kubectl label node k3d-edgeflow-agent-0 k3d-edgeflow-agent-1 k3d-edgeflow-agent-2 edgeflow-role=agent --overwrite

echo "==> importing images into cluster..."
k3d image import edgeflow-server:dev edgeflow-inference:dev-ort -c edgeflow

# ── deploy ────────────────────────────────────────────────────────────────────
echo "==> deploying manifests..."
kubectl apply -f deploy/manifests/

echo "==> waiting for edgeflow-server to be ready..."
kubectl rollout status deployment/edgeflow-server --timeout=120s

# ── train & deploy ────────────────────────────────────────────────────────────
# The inference pod is already starting and will register with the server.
# Creating a deployment here triggers POST /upgrade on the registered pod.
echo "==> running training script..."
EDGEFLOW_SERVER=http://localhost:5000 \
EDGEFLOW_TARGET=iris-inference \
    uv run --project scripts python scripts/train_iris.py

# ── wait for inference to become healthy ──────────────────────────────────────
# The readiness probe on /health only passes after the model is loaded and the
# deployment is confirmed HEALTHY — so rollout status = model is serving.
echo "==> waiting for edgeflow-inference to be ready..."
kubectl rollout status deployment/edgeflow-inference-iris-inference --timeout=300s

# ── smoke test ────────────────────────────────────────────────────────────────
# Brief pause: kubectl rollout completes when the readiness probe passes, but
# k3d's NodePort routing takes a moment to propagate after that.
sleep 3

echo ""
echo "==> smoke test (setosa sample: sepal=5.1x3.5 petal=1.4x0.2)..."
python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \
    | curl -s -X POST http://localhost:8080/infer --data-binary @- \
    | python3 -m json.tool

echo ""
echo "==> deployment state:"
curl -s "http://localhost:5000/api/v1/deployments/latest?target=iris-inference" | python3 -m json.tool

echo ""
echo "done. cluster is up at localhost:5000 (server) and localhost:8080 (inference)."
echo "to tear down: k3d cluster delete edgeflow"
