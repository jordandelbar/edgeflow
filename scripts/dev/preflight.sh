#!/usr/bin/env bash
# Verify required tools and a reachable docker daemon.
set -euo pipefail

require() {
    local cmd=$1 url=$2
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "error: '$cmd' not found in PATH"
        echo "       install: $url"
        exit 1
    fi
}
require docker  https://docs.docker.com/get-docker/
require kubectl https://kubernetes.io/docs/tasks/tools/
require k3d     https://k3d.io/

if ! docker info >/dev/null 2>&1; then
    echo "error: docker daemon not reachable - is the daemon running?"
    exit 1
fi
