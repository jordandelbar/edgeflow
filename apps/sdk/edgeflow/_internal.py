"""
Shared internals for the SDK ops modules.
"""

from __future__ import annotations

import os

from edgeflow import _lib


def client(server: str | None) -> _lib.Client:
    """Resolve the server URL and construct a Rust client.

    Resolution order: explicit ``server`` kwarg, then ``EDGEFLOW_SERVER``
    env var. Raises if neither is set - we deliberately do not fall back
    to a hardcoded localhost value because silent fallbacks hide config
    mistakes (the user thinks they're hitting their server, they're
    actually hitting localhost and seeing a connection-refused).
    """
    resolved = server or os.environ.get("EDGEFLOW_SERVER")
    if not resolved:
        raise RuntimeError(
            "no edgeflow server configured: pass server='http://...' or set "
            "EDGEFLOW_SERVER"
        )
    return _lib.Client(resolved)
