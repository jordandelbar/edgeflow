#!/usr/bin/env bash
# Build the server and inference docker images via docker buildx bake.
set -euo pipefail
docker buildx bake -f deploy/docker-bake.hcl server inference-ort
