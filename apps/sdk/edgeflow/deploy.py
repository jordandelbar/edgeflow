"""
Programmatic register/deploy API.
"""

from __future__ import annotations

import time
from dataclasses import dataclass

from edgeflow._internal import client

TERMINAL_STATES = {"deployed", "failed", "superseded"}


@dataclass
class ModelVersion:
    """A registered model version returned by :func:`register`."""

    name: str
    version: str
    run_id: str | None = None
    current_stage: str = "None"


@dataclass
class Deployment:
    """A deployment returned by :func:`deploy`."""

    deployment_id: str
    run_id: str
    target: str
    state: str
    created_at: int
    model_name: str | None = None
    model_version: str | None = None


def _deployment_from_dict(d: dict) -> Deployment:
    return Deployment(
        deployment_id=d["deployment_id"],
        run_id=d["run_id"],
        target=d["target"],
        state=d["state"],
        created_at=int(d["created_at"]),
        model_name=d.get("model_name"),
        model_version=d.get("model_version"),
    )


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
        A :class:`ModelVersion` with ``.name``, ``.version``, ``.run_id``.
    """
    res = client(server).register_model(run_id, name)
    mv = res.get("model_version", res)

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
    sessions: int | None = None,
    max_concurrent: int | None = None,
    wait: bool = True,
    timeout: int = 300,
) -> Deployment:
    """Deploy a registered model version to an inference target.

    Args:
        model_name:     Registered model name.
        model_version:  Version number (as returned by :func:`register`).
        target:         Inference target name (e.g. ``iris-inference``).
        server:         edgeflow server URL.  Defaults to ``EDGEFLOW_SERVER``
                        env var, then ``http://localhost:5000``.
        sessions:       Initial pool size to request. Optional - server picks
                        a default if omitted. Mirrors ``edgeflow deploy --sessions``.
        max_concurrent: Initial max in-flight cap before 429. Optional -
                        defaults to ``sessions`` server-side.
        wait:           Block until the deployment reaches a terminal state.
        timeout:        Seconds to wait when ``wait=True``.

    Returns:
        The final :class:`Deployment`.

    Raises:
        RuntimeError: If the deployment fails or times out.
    """
    api = client(server)

    print(f"🚀 Deploying {model_name} v{model_version} → target '{target}'")

    res = api.create_deployment(
        model_name, model_version, target, sessions, max_concurrent
    )
    dep = res["deployment"]
    deployment_id = dep["deployment_id"]

    print(f"   deployment_id: {deployment_id}")

    if not wait:
        return _deployment_from_dict(dep)

    deadline = time.monotonic() + timeout
    last_state = dep["state"]

    while time.monotonic() < deadline:
        time.sleep(2)
        res = api.get_deployment(deployment_id)
        dep = res["deployment"]
        state = dep["state"]
        if state != last_state:
            print(f"   {last_state} → {state}")
            last_state = state
        if state in TERMINAL_STATES:
            break

    state = dep["state"]
    if state == "deployed":
        print(f"✅ Deployment live on '{target}'")
        return _deployment_from_dict(dep)
    if time.monotonic() >= deadline:
        raise RuntimeError(
            f"deployment {deployment_id} timed out after {timeout}s - last state: {state}"
        )
    raise RuntimeError(f"deployment {deployment_id} ended in state: {state}")
