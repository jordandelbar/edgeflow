"""
Typed dataclasses for edgeflow REST responses.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

# ── Resources / infra ────────────────────────────────────────────────────────


@dataclass
class ResourceSettings:
    """Edgeflow-owned settings, persisted in SQLite."""

    sessions: int | None = None
    max_concurrent: int | None = None


@dataclass
class InfraSettings:
    """k8s-owned infrastructure settings, read from the Deployment spec."""

    cpu_request: str | None = None
    memory_request: str | None = None
    memory_limit: str | None = None
    replicas: int | None = None
    placement: str | None = None
    node_selector: dict[str, str] | None = None


# ── Targets ──────────────────────────────────────────────────────────────────


@dataclass
class TargetPod:
    pod_id: str
    address: str
    node: str | None
    registered_at: int
    health: str  # "healthy" | "stale" | "unhealthy" | "unknown"


@dataclass
class Target:
    target: str
    registered_at: int
    health: str
    resources: ResourceSettings
    infra: InfraSettings | None = None
    pods: list[TargetPod] = field(default_factory=list)
    current_run_id: str | None = None
    model_loaded_at: str | None = None
    node: str | None = None


def target_from_dict(d: dict) -> Target:
    res = d.get("resources") or {}
    inf = d.get("infra")
    pods = [
        TargetPod(
            pod_id=p["pod_id"],
            address=p["address"],
            node=p.get("node"),
            registered_at=int(p["registered_at"]),
            health=p["health"],
        )
        for p in d.get("pods", []) or []
    ]
    return Target(
        target=d["target"],
        registered_at=int(d["registered_at"]),
        health=d.get("health", "unknown"),
        resources=ResourceSettings(
            sessions=res.get("sessions"),
            max_concurrent=res.get("max_concurrent"),
        ),
        infra=(
            InfraSettings(
                cpu_request=inf.get("cpu_request"),
                memory_request=inf.get("memory_request"),
                memory_limit=inf.get("memory_limit"),
                replicas=inf.get("replicas"),
                placement=inf.get("placement"),
                node_selector=inf.get("node_selector"),
            )
            if inf is not None
            else None
        ),
        pods=pods,
        current_run_id=d.get("current_run_id"),
        model_loaded_at=d.get("model_loaded_at"),
        node=d.get("node"),
    )


# ── Runs / Experiments / Registry ────────────────────────────────────────────


@dataclass
class Run:
    """Subset of the MLflow run shape; ``data`` keeps the full response for
    callers that need params/metrics/tags."""

    run_id: str
    experiment_id: str
    status: str
    start_time: int
    end_time: int | None
    artifact_uri: str
    run_name: str | None = None
    data: dict[str, Any] = field(default_factory=dict)


def run_from_dict(d: dict) -> Run:
    info = d.get("info", d)
    return Run(
        run_id=info["run_id"],
        experiment_id=info["experiment_id"],
        status=info["status"],
        start_time=int(info["start_time"]),
        end_time=int(info["end_time"]) if info.get("end_time") is not None else None,
        artifact_uri=info["artifact_uri"],
        run_name=info.get("run_name"),
        data=d.get("data", {}),
    )


@dataclass
class Experiment:
    experiment_id: str
    name: str
    artifact_location: str
    lifecycle_stage: str
    creation_time: int
    last_update_time: int


def experiment_from_dict(d: dict) -> Experiment:
    return Experiment(
        experiment_id=d["experiment_id"],
        name=d["name"],
        artifact_location=d["artifact_location"],
        lifecycle_stage=d["lifecycle_stage"],
        creation_time=int(d["creation_time"]),
        last_update_time=int(d["last_update_time"]),
    )


@dataclass
class RegisteredModel:
    name: str
    creation_time: int
    last_updated_time: int
    description: str | None = None


def registered_model_from_dict(d: dict) -> RegisteredModel:
    return RegisteredModel(
        name=d["name"],
        creation_time=int(d["creation_time"]),
        last_updated_time=int(d["last_updated_time"]),
        description=d.get("description"),
    )
