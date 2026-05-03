"""Parity tests for read-only commands.

Each test runs the same operation through both surfaces (CLI `--json` and the
SDK's underlying client) and asserts the JSON payloads are byte-equivalent
(modulo dict-key ordering, which `==` already handles for dicts).

The point of these tests is to lock the JSON contract surface: when someone
adds a field to one surface and forgets the other, this catches it before
users notice.
"""

from __future__ import annotations

import pytest

SERVER_URL = "http://localhost:5000"


def _sdk_raw(method_name: str, *args):
    """Call the underlying Rust client directly so we compare apples to
    apples - the SDK's typed wrappers (Target, Run, ...) drop / rename
    fields, which is a separate contract worth pinning later."""
    from edgeflow._internal import client

    return getattr(client(SERVER_URL), method_name)(*args)


@pytest.mark.parametrize(
    "cli_args, sdk_method",
    [
        (("nodes", "list"), "list_nodes"),
        (("targets", "list"), "list_targets"),
        (("experiments", "list"), "list_experiments"),
        (("models", "list"), "list_registered_models"),
        (("deployments", "list"), "list_deployments"),
    ],
    ids=["nodes", "targets", "experiments", "models", "deployments"],
)
def test_list_command_parity(k3d_stack, cli_json, cli_args, sdk_method):
    """Every list command returns the same payload via CLI --json and SDK."""
    cli_payload = cli_json(*cli_args)
    sdk_payload = _sdk_raw(sdk_method)
    assert cli_payload == sdk_payload, (
        f"divergence between `edgeflow {' '.join(cli_args)} --json` and "
        f"`client.{sdk_method}()`:\nCLI: {cli_payload}\nSDK: {sdk_payload}"
    )


def test_deployments_list_filtered_parity(k3d_stack, cli_json):
    """Optional --target filter should reach the same endpoint on both sides."""
    cli_payload = cli_json("deployments", "list", "--target", "nonexistent-target")
    sdk_payload = _sdk_raw("list_deployments", "nonexistent-target")
    assert cli_payload == sdk_payload
