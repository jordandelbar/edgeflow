"""Target management."""

from __future__ import annotations

from edgeflow._internal import client
from edgeflow._types import Target, target_from_dict


def list(*, server: str | None = None) -> list[Target]:
    """List all registered targets."""
    res = client(server).list_targets()
    return [target_from_dict(t) for t in res.get("targets", [])]


def get(target: str, *, server: str | None = None) -> Target:
    """Fetch full details for a target."""
    res = client(server).get_target(target)
    return target_from_dict(res["target"])


def set_resources(
    target: str,
    *,
    sessions: int | None = None,
    max_concurrent: int | None = None,
    cpu_request: str | None = None,
    memory_request: str | None = None,
    memory_limit: str | None = None,
    replicas: int | None = None,
    placement: str | None = None,
    server: str | None = None,
) -> Target:
    """Update resource settings (merges with existing values)."""
    res = client(server).update_target_resources(
        target,
        sessions,
        max_concurrent,
        cpu_request,
        memory_request,
        memory_limit,
        replicas,
        placement,
    )
    return target_from_dict(res["target"])


def teardown(target: str, *, server: str | None = None) -> None:
    """Tear down a target (removes pod and deployment record)."""
    client(server).teardown_target(target)
