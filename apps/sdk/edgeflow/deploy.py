"""Programmatic deploy API — used by both the CLI and training scripts."""

import os
import time

import requests

TERMINAL_STATES = {"deployed", "failed", "superseded"}
_DEFAULT_SERVER = "http://localhost:5000"


def deploy(
    run_id: str,
    target: str,
    *,
    server: str | None = None,
    wait: bool = True,
    timeout: int = 300,
) -> dict:
    """Deploy a run to an inference target.

    Args:
        run_id:  MLflow run ID to deploy.
        target:  Inference target name (e.g. ``iris-inference``).
        server:  edgeflow server URL. Defaults to ``EDGEFLOW_SERVER`` env var,
                 then ``http://localhost:5000``.
        wait:    Block until the deployment reaches a terminal state.
        timeout: Seconds to wait when ``wait=True``.

    Returns:
        The final deployment dict from the server.

    Raises:
        RuntimeError: If the deployment fails or times out.
    """
    server = server or os.environ.get("EDGEFLOW_SERVER", _DEFAULT_SERVER)

    resp = requests.post(
        f"{server}/api/v1/deployments",
        json={"run_id": run_id, "target": target},
        timeout=10,
    )
    resp.raise_for_status()
    deployment = resp.json()["deployment"]

    if not wait:
        return deployment

    deployment_id = deployment["deployment_id"]
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        time.sleep(2)
        resp = requests.get(f"{server}/api/v1/deployments/{deployment_id}", timeout=10)
        resp.raise_for_status()
        deployment = resp.json()["deployment"]
        if deployment["state"] in TERMINAL_STATES:
            break

    state = deployment["state"]
    if state == "deployed":
        return deployment
    if time.monotonic() >= deadline:
        raise RuntimeError(
            f"deployment {deployment_id} timed out after {timeout}s — last state: {state}"
        )
    raise RuntimeError(f"deployment {deployment_id} ended in state: {state}")
