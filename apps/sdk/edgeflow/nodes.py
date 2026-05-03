"""Cluster node listing."""

from __future__ import annotations

from edgeflow._internal import client


def list(*, server: str | None = None) -> list[str]:
    """List cluster node names."""
    res = client(server).list_nodes()
    return [str(n) for n in res.get("nodes", [])]
