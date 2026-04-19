"""
Edgeflow inference load test.

The script resolves the pod address from the edgeflow server at startup,
then drives POST /infer directly - no server proxy overhead.

Environment variables
---------------------
EDGEFLOW_SERVER   Base URL of the edgeflow server  (default: http://localhost:5000)
EDGEFLOW_TARGET   Target name to benchmark          (required)
PAYLOAD_FILE      Path to request body file         (required)
CONTENT_TYPE      Content-type header               (default: application/octet-stream)

If locust is CPU-bound (server answers faster than locust can generate load),
add --processes to spread across cores:
    locust -f locustfile.py ... --processes 4

Quick-start examples (after `make_payloads.py`)
-----------------------------------------------
# iris (raw floats)
EDGEFLOW_TARGET=iris-inference \
PAYLOAD_FILE=payloads/iris.bin \
locust -f locustfile.py --headless -u 10 -r 2 -t 60s

# adult income (JSON)
EDGEFLOW_TARGET=adult-inference \
PAYLOAD_FILE=payloads/adult.json \
CONTENT_TYPE=application/json \
locust -f locustfile.py --headless -u 10 -r 2 -t 60s

# yolov8 (JPEG)
EDGEFLOW_TARGET=yolo-inference \
PAYLOAD_FILE=payloads/sample.jpg \
CONTENT_TYPE=image/jpeg \
locust -f locustfile.py --headless -u 4 -r 1 -t 60s
"""

import os

import requests
from locust import between, events, task
from locust.contrib.fasthttp import FastHttpUser

# ── configuration ────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
TARGET = os.environ.get("EDGEFLOW_TARGET", "")
PAYLOAD_FILE = os.environ.get("PAYLOAD_FILE", "")
CONTENT_TYPE = os.environ.get("CONTENT_TYPE", "application/octet-stream")
# Override the resolved pod address (e.g. when port-forwarding from outside the cluster).
INFER_HOST = os.environ.get("INFER_HOST", "")
ENDPOINT = "/infer"


def _resolve_pod_address(server: str, target: str) -> str:
    resp = requests.get(f"{server}/api/v1/targets", timeout=5)
    resp.raise_for_status()
    targets = resp.json().get("targets", [])
    for t in targets:
        if t["target"] == target:
            pods = t.get("pods", [])
            if not pods:
                raise SystemExit(f"[locust] target '{target}' has no registered pods")
            addr = pods[0]["address"]
            if not addr.startswith("http"):
                addr = f"http://{addr}"
            print(f"[locust] target '{target}' → {addr}")
            return addr
    raise SystemExit(
        f"[locust] target '{target}' not registered on {server}.\n"
        "  Available targets: " + ", ".join(t["target"] for t in targets)
    )


def _load_payload(path: str) -> bytes:
    if not path:
        raise SystemExit("[locust] PAYLOAD_FILE is required")
    with open(path, "rb") as f:
        data = f.read()
    print(
        f"[locust] payload: {path!r} ({len(data)} bytes, content-type: {CONTENT_TYPE})"
    )
    return data


# Resolve once at import time so FastHttpUser.host can be set at class level.
if not TARGET:
    raise SystemExit("[locust] EDGEFLOW_TARGET is required")

_pod_address = (
    INFER_HOST if INFER_HOST else _resolve_pod_address(EDGEFLOW_SERVER, TARGET)
)
_payload = _load_payload(PAYLOAD_FILE)

# ── user ─────────────────────────────────────────────────────────────────────


class InferenceUser(FastHttpUser):
    # Locust's --host flag is ignored; we use the resolved pod address.
    host = _pod_address

    # No artificial wait - we want to measure raw throughput ceiling.
    wait_time = between(0, 0)

    @task
    def infer(self):
        with self.client.post(
            ENDPOINT,
            data=_payload,
            headers={"content-type": CONTENT_TYPE},
            catch_response=True,
        ) as resp:
            if resp.status_code == 200:
                resp.success()
            elif resp.status_code == 429:
                # Backpressure from the semaphore - expected under saturation.
                # Mark success so it doesn't inflate the failure rate.
                resp.success()
            elif resp.status_code == 503:
                # Model not loaded yet - fail visibly.
                resp.failure("503 no model loaded")
            else:
                resp.failure(
                    f"unexpected {resp.status_code}: {(resp.text or '')[:120]}"
                )


# ── startup banner ───────────────────────────────────────────────────────────


@events.test_start.add_listener
def on_test_start(environment, **_kwargs):
    print(
        f"\n{'─' * 60}\n"
        f"  target      : {TARGET}\n"
        f"  pod address : {_pod_address}\n"
        f"  payload     : {PAYLOAD_FILE} ({len(_payload)} B)\n"
        f"  content-type: {CONTENT_TYPE}\n"
        f"{'─' * 60}\n"
    )
