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

echo "==> pushing images to local registry..."
just push

# ── deploy ────────────────────────────────────────────────────────────────────
echo "==> deploying manifests..."
kubectl apply -f deploy/manifests/

echo "==> waiting for observability stack to be ready..."
kubectl rollout status deployment/otelcol    --timeout=120s
kubectl rollout status deployment/prometheus --timeout=120s
kubectl rollout status deployment/tempo      --timeout=120s
kubectl rollout status deployment/grafana    --timeout=120s

echo "==> waiting for edgeflow-server to be ready..."
kubectl rollout status deployment/edgeflow-server --timeout=120s
