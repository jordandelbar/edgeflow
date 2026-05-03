#!/usr/bin/env bash
# Create the edgeflow k3d cluster and label its nodes.
set -euo pipefail

attempts=3
for i in $(seq 1 $attempts); do
    if k3d cluster create --config deploy/k3d-cluster.yaml; then
        break
    fi
    if [ "$i" -eq "$attempts" ]; then
        echo "k3d cluster create failed after $attempts attempts" >&2
        exit 1
    fi
    echo "k3d cluster create failed (attempt $i/$attempts), cleaning up and retrying" >&2
    k3d cluster delete edgeflow >/dev/null 2>&1 || true
    sleep 5
done

kubectl label node k3d-edgeflow-server-0 \
    edgeflow-role=server --overwrite
kubectl label node k3d-edgeflow-agent-0 k3d-edgeflow-agent-1 k3d-edgeflow-agent-2 \
    edgeflow-role=agent --overwrite
