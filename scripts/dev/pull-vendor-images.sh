#!/usr/bin/env bash
# Pull observability images from upstream and tag them for the local registry.
set -euo pipefail

REGISTRY="localhost:5001"

declare -A images=(
    ["otel/opentelemetry-collector-contrib:0.145.0"]="$REGISTRY/otel-collector-contrib:0.145.0"
    ["grafana/tempo:2.7.2"]="$REGISTRY/grafana/tempo:2.7.2"
    ["grafana/grafana:11.6.1"]="$REGISTRY/grafana:11.6.1"
    ["prom/prometheus:v3.3.0"]="$REGISTRY/prometheus:v3.3.0"
)
for src in "${!images[@]}"; do
    dst="${images[$src]}"
    echo "Pulling $src..."
    docker pull "$src"
    echo "Tagging as $dst..."
    docker tag "$src" "$dst"
done
