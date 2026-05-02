"""Run lookups."""

from __future__ import annotations

from edgeflow._internal import client
from edgeflow._types import Run, run_from_dict


def get(run_id: str, *, server: str | None = None) -> Run:
    """Fetch a run by ID."""
    res = client(server).get_run(run_id)
    return run_from_dict(res.get("run", res))
