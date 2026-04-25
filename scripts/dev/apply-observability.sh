#!/usr/bin/env bash
# Apply the observability manifest and wait for all components to be ready.
set -euo pipefail

kubectl apply -f deploy/manifests/observability.yaml
kubectl rollout status deployment/otelcol    --timeout=120s
kubectl rollout status deployment/prometheus --timeout=120s
kubectl rollout status deployment/tempo      --timeout=120s
kubectl rollout status deployment/grafana    --timeout=120s
