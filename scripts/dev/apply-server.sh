#!/usr/bin/env bash
# Apply the edgeflow-server manifest and wait for it to be ready.
set -euo pipefail

kubectl apply -f deploy/manifests/server.yaml
kubectl rollout status deployment/edgeflow-server --timeout=120s
