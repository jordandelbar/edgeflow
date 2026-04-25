#!/usr/bin/env bash
# Create the edgeflow k3d cluster and label its nodes.
set -euo pipefail

k3d cluster create --config deploy/k3d-cluster.yaml

kubectl label node k3d-edgeflow-server-0 \
    edgeflow-role=server --overwrite
kubectl label node k3d-edgeflow-agent-0 k3d-edgeflow-agent-1 k3d-edgeflow-agent-2 \
    edgeflow-role=agent --overwrite
