#!/usr/bin/env bash
# Push vendor + app images to the local registry.
set -euo pipefail

REGISTRY="localhost:5001"

# Vendored observability images
for img in \
    "$REGISTRY/otel-collector-contrib:0.145.0" \
    "$REGISTRY/prometheus:v3.3.0" \
    "$REGISTRY/grafana/tempo:2.7.2" \
    "$REGISTRY/grafana:11.6.1"; do
    echo "Pushing $img..."
    docker push "$img"
done

# App images
for img in edgeflow-server:dev edgeflow-inference:dev-ort; do
    echo "Tagging and pushing $REGISTRY/$img..."
    docker tag "$img" "$REGISTRY/$img"
    docker push "$REGISTRY/$img"
done
