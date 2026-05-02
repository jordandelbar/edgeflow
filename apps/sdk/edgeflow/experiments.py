"""Experiment queries."""

from __future__ import annotations

from edgeflow._internal import client
from edgeflow._types import Experiment, Run, experiment_from_dict, run_from_dict


def list(*, server: str | None = None) -> list[Experiment]:
    """List all experiments."""
    res = client(server).list_experiments()
    return [experiment_from_dict(e) for e in res.get("experiments", [])]


def runs(experiment_id: str, *, server: str | None = None) -> list[Run]:
    """List runs in an experiment."""
    res = client(server).search_runs(experiment_id)
    return [run_from_dict(r) for r in res.get("runs", [])]
