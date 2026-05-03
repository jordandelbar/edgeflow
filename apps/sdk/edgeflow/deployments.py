"""Deployment queries."""

from __future__ import annotations

from edgeflow._internal import client
from edgeflow.deploy import Deployment, _deployment_from_dict


def list(target: str | None = None, *, server: str | None = None) -> list[Deployment]:
    """
    List deployments, optionally filtered by target.
    """
    res = client(server).list_deployments(target)
    return [_deployment_from_dict(d) for d in res.get("deployments", [])]


def status(target: str, *, server: str | None = None) -> Deployment:
    """Latest deployment for a target."""
    res = client(server).latest_deployment(target)
    return _deployment_from_dict(res["deployment"])


def get(deployment_id: str, *, server: str | None = None) -> Deployment:
    """Fetch a specific deployment by id."""
    res = client(server).get_deployment(deployment_id)
    return _deployment_from_dict(res["deployment"])
