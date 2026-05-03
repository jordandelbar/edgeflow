#!/usr/bin/env bash
# Push images to the local registry.
#
# Usage:
#   push-images.sh             # default: all (app + observability)
#   push-images.sh app         # just edgeflow-server + edgeflow-inference
#   push-images.sh observability  # just the observability stack
#   push-images.sh all         # both
#
# CI flows that don't care about observability (e.g. the k3d smoke test)
# pass `app` to skip the heavier vendor-image pull/push.

set -euo pipefail

REGISTRY="localhost:5001"
TARGET="${1:-all}"

push_observability() {
    for img in \
        "$REGISTRY/otel-collector-contrib:0.145.0" \
        "$REGISTRY/prometheus:v3.3.0" \
        "$REGISTRY/grafana/tempo:2.7.2" \
        "$REGISTRY/grafana:11.6.1"; do
        echo "Pushing $img..."
        docker push "$img"
    done
}

push_app() {
    for img in edgeflow-server:dev edgeflow-inference:dev-ort; do
        echo "Tagging and pushing $REGISTRY/$img..."
        docker tag "$img" "$REGISTRY/$img"
        docker push "$REGISTRY/$img"
    done
}

case "$TARGET" in
    all)            push_observability; push_app ;;
    app)            push_app ;;
    observability)  push_observability ;;
    *)
        echo "usage: $0 [all|app|observability]" >&2
        exit 1
        ;;
esac
