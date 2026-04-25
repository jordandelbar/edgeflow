#!/usr/bin/env bash
# Quick load-test launcher.
# Usage: ./bench.sh <target> [users] [duration]
#
# Examples
#   ./bench.sh iris-inference
#   ./bench.sh adult-inference 50 120s
#   ./bench.sh yolo-inference 4 30s
#
# The script picks the right payload, content-type, locustfile, and deploy
# script based on the target name. If the target isn't already deployed it
# trains + deploys it first by invoking the matching example's train script
#
# Override any of them via env vars:
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
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$SCRIPT_DIR"

# ── pick locustfile + payload + deploy script based on target name ────────────
case "$TARGET" in
  *adult*)
    LOCUSTFILE="locustfile_adult.py"
    PAYLOAD_FILE=""
    INFER_HOST="http://127.0.0.1:8081"
    CONTENT_TYPE="application/json"
    DEPLOY_SCRIPT="$REPO_ROOT/examples/03-adult-income/train.py"
    ;;
  *yolo*|*image*)
    LOCUSTFILE="locustfile.py"
    PAYLOAD_FILE="payloads/sample.jpg"
    CONTENT_TYPE="image/jpeg"
    DEPLOY_SCRIPT="$REPO_ROOT/examples/05-k3d-yolo/deploy.py"
    ;;
  *)
    # Default: iris-style JSON array body (`[5.1, 3.5, 1.4, 0.2]`)
    LOCUSTFILE="locustfile.py"
    PAYLOAD_FILE="payloads/iris.json"
    CONTENT_TYPE="application/json"
    DEPLOY_SCRIPT="$REPO_ROOT/examples/01-quickstart-iris/train.py"
    ;;
esac

# ── ensure the target is deployed; train + deploy if not ──────────────────────
status=$(curl -s -o /dev/null -w "%{http_code}" \
    "$EDGEFLOW_SERVER/api/v1/targets/$TARGET" || echo "000")
if [[ "$status" != "200" ]]; then
    echo "target '$TARGET' not deployed (status=$status), running $DEPLOY_SCRIPT..."
    EDGEFLOW_SERVER="$EDGEFLOW_SERVER" \
    EDGEFLOW_TARGET="$TARGET" \
    uv run --with-editable "$REPO_ROOT/apps/sdk" "$DEPLOY_SCRIPT"
    echo ""
fi

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
    --only-summary \
    --loglevel WARNING \
    -u "$USERS" -r "$((USERS / 2 < 1 ? 1 : USERS / 2))" \
    -t "$DURATION"
