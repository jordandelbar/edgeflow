#!/usr/bin/env bash
# Delete the edgeflow k3d cluster. Idempotent.
set -euo pipefail

if k3d cluster list | grep -q "^edgeflow"; then
    k3d cluster delete edgeflow
else
    echo "no edgeflow cluster to delete"
fi
