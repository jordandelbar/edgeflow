"""Programmatic deploy/register API - used by both the CLI and training scripts."""

import os
import time
from dataclasses import dataclass

import requests

TERMINAL_STATES = {"deployed", "failed", "superseded"}
_DEFAULT_SERVER = "http://localhost:5000"


@dataclass
class ModelVersion:
    """A registered model version returned by :func:`register`."""

    name: str
    version: str
    run_id: str | None = None
    current_stage: str = "None"


def register(
    run_id: str,
    name: str,
    *,
    server: str | None = None,
) -> ModelVersion:
    """Register a run as a model version in the edgeflow model registry.

    Creates the registered model if it does not exist yet (idempotent).

    Args:
        run_id: Run ID to register.
        name:   Registered model name (e.g. ``"iris-classifier"``).
        server: edgeflow server URL.  Defaults to ``EDGEFLOW_SERVER`` env var,
                then ``http://localhost:5000``.

    Returns:
        A :class:`ModelVersion` with ``.name`` and ``.version``.
    """
    server = server or os.environ.get("EDGEFLOW_SERVER", _DEFAULT_SERVER)

    # Create registered model (idempotent).
    resp = requests.post(
        f"{server}/api/2.0/mlflow/registered-models/create",
        json={"name": name},
        timeout=10,
    )
    if not resp.ok and "already exists" not in resp.text.lower():
        resp.raise_for_status()

    # Create model version linked to this run.
    resp = requests.post(
        f"{server}/api/2.0/mlflow/model-versions/create",
        json={"name": name, "run_id": run_id},
        timeout=10,
    )
    resp.raise_for_status()
    mv = resp.json().get("model_version", {})

    print(f"📦 Registered {name} v{mv['version']}")
    return ModelVersion(
        name=mv["name"],
        version=str(mv["version"]),
        run_id=mv.get("run_id"),
        current_stage=mv.get("current_stage", "None"),
    )


def deploy(
    model_name: str,
    model_version: str,
    target: str,
    *,
    server: str | None = None,
    wait: bool = True,
    timeout: int = 300,
) -> dict:
    """Deploy a registered model version to an inference target.

    Args:
        model_name:    Registered model name.
        model_version: Version number (as returned by :func:`register`).
        target:        Inference target name (e.g. ``iris-inference``).
        server:        edgeflow server URL.  Defaults to ``EDGEFLOW_SERVER``
                       env var, then ``http://localhost:5000``.
        wait:          Block until the deployment reaches a terminal state.
        timeout:       Seconds to wait when ``wait=True``.

    Returns:
        The final deployment dict from the server.

    Raises:
        RuntimeError: If the deployment fails or times out.
    """
    server = server or os.environ.get("EDGEFLOW_SERVER", _DEFAULT_SERVER)

    print(f"🚀 Deploying {model_name} v{model_version} → target '{target}'")

    resp = requests.post(
        f"{server}/api/v1/deployments",
        json={
            "model_name": model_name,
            "model_version": model_version,
            "target": target,
        },
        timeout=10,
    )
    resp.raise_for_status()
    deployment = resp.json()["deployment"]
    deployment_id = deployment["deployment_id"]

    print(f"   deployment_id: {deployment_id}")

    if not wait:
        return deployment

    deadline = time.monotonic() + timeout
    last_state = deployment["state"]

    while time.monotonic() < deadline:
        time.sleep(2)
        resp = requests.get(f"{server}/api/v1/deployments/{deployment_id}", timeout=10)
        resp.raise_for_status()
        deployment = resp.json()["deployment"]
        state = deployment["state"]
        if state != last_state:
            print(f"   {last_state} → {state}")
            last_state = state
        if state in TERMINAL_STATES:
            break

    state = deployment["state"]
    if state == "deployed":
        print(f"✅ Deployment live on '{target}'")
        return deployment
    if time.monotonic() >= deadline:
        raise RuntimeError(
            f"deployment {deployment_id} timed out after {timeout}s - last state: {state}"
        )
    raise RuntimeError(f"deployment {deployment_id} ended in state: {state}")
