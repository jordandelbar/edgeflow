#!/usr/bin/env bash
# Apply the inference Services. Pods are created on demand by the server when
# a deployment is requested, so there is nothing to wait for here.
set -euo pipefail

kubectl apply -f deploy/manifests/inference.yaml
