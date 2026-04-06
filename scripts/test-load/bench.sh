#!/usr/bin/env bash
# Quick load-test launcher.
# Usage: ./bench.sh <target> [users] [duration]
#
# Examples
#   ./bench.sh iris-inference
#   ./bench.sh adult-inference 50 120s
#   ./bench.sh yolo-inference 4 30s
#
# The script picks the right payload, content-type, and locustfile
# based on the target name.  Override any of them via env vars:
#   INFER_HOST      direct pod/NodePort URL (default: http://localhost:8080)
#   EDGEFLOW_SERVER edgeflow server URL     (default: http://localhost:5000)
#   LOCUST_USERS    virtual users           (overrides positional arg)
#   LOCUST_DURATION run duration            (overrides positional arg)

set -euo pipefail

TARGET="${1:?usage: $0 <target> [users] [duration]}"
USERS="${LOCUST_USERS:-${2:-10}}"
DURATION="${LOCUST_DURATION:-${3:-60s}}"

INFER_HOST="${INFER_HOST:-http://127.0.0.1:8080}"
EDGEFLOW_SERVER="${EDGEFLOW_SERVER:-http://localhost:5000}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# ── pick locustfile + payload based on target name ────────────────────────────
case "$TARGET" in
  *adult*)
    LOCUSTFILE="locustfile_adult.py"
    PAYLOAD_FILE=""
    INFER_HOST="http://127.0.0.1:8081"
    CONTENT_TYPE="application/json"
    ;;
  *yolo*|*image*)
    LOCUSTFILE="locustfile.py"
    PAYLOAD_FILE="payloads/sample.jpg"
    CONTENT_TYPE="image/jpeg"
    ;;
  *)
    # Default: iris-style raw float binary
    LOCUSTFILE="locustfile.py"
    PAYLOAD_FILE="payloads/iris.bin"
    CONTENT_TYPE="application/octet-stream"
    ;;
esac

# ── check payloads exist ──────────────────────────────────────────────────────
if [[ -n "$PAYLOAD_FILE" && ! -f "$PAYLOAD_FILE" ]]; then
    echo "payload not found: $PAYLOAD_FILE"
    echo "run: uv run make_payloads.py"
    exit 1
fi

# ── run ───────────────────────────────────────────────────────────────────────
echo "target:   $TARGET"
echo "host:     $INFER_HOST"
echo "users:    $USERS  duration: $DURATION"
echo ""

EDGEFLOW_SERVER="$EDGEFLOW_SERVER" \
EDGEFLOW_TARGET="$TARGET" \
INFER_HOST="$INFER_HOST" \
PAYLOAD_FILE="$PAYLOAD_FILE" \
CONTENT_TYPE="$CONTENT_TYPE" \
uv run locust -f "$LOCUSTFILE" \
    --headless \
    -u "$USERS" -r "$((USERS / 2 < 1 ? 1 : USERS / 2))" \
    -t "$DURATION"
