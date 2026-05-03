"""Shared fixtures for tutorial and parity e2e tests.

Two stack flavors live here:

- `edgeflow_stack` (docker-compose) - boots the dev compose file so tutorial
  tests exercise the same code paths a tutorial reader would. Set
  `EDGEFLOW_E2E_SKIP_STACK=1` to reuse a stack you started yourself.
- `k3d_stack` (kubernetes) - assumes a reachable k3d cluster + edgeflow
  server already running (e.g. via `just up`). Parity tests need this
  because they assert against `kubectl` for cluster-state side effects.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import time
import uuid
from pathlib import Path

import pytest
import requests

REPO_ROOT = Path(__file__).resolve().parents[2]
COMPOSE_FILE = REPO_ROOT / "deploy" / "docker-compose.yaml"
SERVER_URL = "http://localhost:5000"
INFERENCE_URL = "http://localhost:8080"

STACK_BOOT_TIMEOUT = 90.0


def _log(msg: str) -> None:
    print(f"[e2e] {msg}", flush=True)


def _wait_until_reachable(url: str, timeout: float) -> None:
    _log(f"waiting for {url} (up to {timeout:.0f}s)")
    deadline = time.monotonic() + timeout
    last_err: Exception | None = None
    while time.monotonic() < deadline:
        try:
            resp = requests.get(url, timeout=2)
            if resp.status_code < 500:
                _log(f"reachable: {url} ({resp.status_code})")
                return
            last_err = RuntimeError(f"{url} returned {resp.status_code}")
        except requests.RequestException as exc:
            last_err = exc
        time.sleep(1)
    raise RuntimeError(f"timed out waiting for {url}: {last_err}")


def _compose(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["docker", "compose", "-f", str(COMPOSE_FILE), *args],
        cwd=REPO_ROOT,
        check=check,
        text=True,
    )


@pytest.fixture(scope="session")
def edgeflow_stack(request: pytest.FixtureRequest):
    if os.environ.get("EDGEFLOW_E2E_SKIP_STACK"):
        _log("EDGEFLOW_E2E_SKIP_STACK set, using existing stack")
        yield
        return

    if os.environ.get("EDGEFLOW_E2E_SKIP_BUILD"):
        _log("EDGEFLOW_E2E_SKIP_BUILD set, using preloaded images")
    else:
        _log(
            f"building images via {COMPOSE_FILE.relative_to(REPO_ROOT)} (cold builds take several minutes)"
        )
        _compose("build")
    _log("starting stack")
    _compose("up", "-d")
    fixture_failed = False
    try:
        _wait_until_reachable(SERVER_URL, STACK_BOOT_TIMEOUT)
        _wait_until_reachable(INFERENCE_URL, STACK_BOOT_TIMEOUT)
        _log("stack ready")
        yield
    except Exception:
        fixture_failed = True
        raise
    finally:
        tests_failed = request.session.testsfailed > 0
        if fixture_failed or tests_failed or os.environ.get("EDGEFLOW_E2E_DUMP_LOGS"):
            _log("dumping stack logs")
            _compose("logs", "--no-color", check=False)
        _log("tearing down stack")
        _compose("down", "-v", check=False)


@pytest.fixture(scope="session")
def server_url() -> str:
    return SERVER_URL


@pytest.fixture(scope="session")
def inference_url() -> str:
    return INFERENCE_URL


@pytest.fixture(scope="session")
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture(scope="session")
def local_sdk_wheel(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Build the local SDK as a wheel pinned to `999.0.0` so it strictly
    outranks any PyPI release of `edgeflow` in uv's resolver. Tests then run
    `uv run --find-links <wheel-dir> train.py` and uv deterministically picks
    the local wheel. (PEP 440 local-version segments like `+e2e` are not
    enough - uv tends to prefer canonical versions of the same base.)
    Patches apps/sdk/pyproject.toml in place for the build and restores it
    afterwards."""
    transforms_dir = REPO_ROOT / "crates" / "transforms"
    sdk_dir = REPO_ROOT / "apps" / "sdk"
    sdk_pyproject = sdk_dir / "pyproject.toml"

    _log("building wasm transform (cargo build --target wasm32-wasip2 --release)")
    subprocess.run(
        ["cargo", "build", "--target", "wasm32-wasip2", "--release"],
        cwd=transforms_dir,
        check=True,
    )

    _log("staging _lib.wasm into apps/sdk/edgeflow/wasm/")
    wasm_dst = sdk_dir / "edgeflow" / "wasm"
    wasm_dst.mkdir(parents=True, exist_ok=True)
    shutil.copy(
        transforms_dir / "target" / "wasm32-wasip2" / "release" / "_lib.wasm",
        wasm_dst / "standard_pipeline.wasm",
    )

    original_pyproject = sdk_pyproject.read_text()
    bumped_pyproject, n = re.subn(
        r'^version = "[^"]+"',
        'version = "999.0.0"',
        original_pyproject,
        count=1,
        flags=re.MULTILINE,
    )
    if n != 1:
        raise RuntimeError(
            "could not find SDK version line in apps/sdk/pyproject.toml to patch"
        )
    sdk_pyproject.write_text(bumped_pyproject)

    wheels_dir = tmp_path_factory.mktemp("sdk-wheel")
    try:
        _log(f"building sdk wheel via maturin into {wheels_dir}")
        subprocess.run(
            [
                "uv",
                "run",
                "--with",
                "maturin",
                "maturin",
                "build",
                "--release",
                "--features",
                "python",
                "--out",
                str(wheels_dir),
            ],
            cwd=sdk_dir,
            check=True,
        )
    finally:
        sdk_pyproject.write_text(original_pyproject)

    wheels = list(wheels_dir.glob("edgeflow-*.whl"))
    if len(wheels) != 1:
        raise RuntimeError(
            f"expected exactly 1 edgeflow wheel, found {len(wheels)} in {wheels_dir}"
        )
    _log(f"sdk wheel ready: {wheels[0].name}")
    return wheels[0]


# ── Parity-test fixtures (CLI <-> SDK) ──────────────────────────────────────


@pytest.fixture(scope="session")
def k3d_stack():
    """Assert a reachable edgeflow server (assumed k3d-backed). Parity tests
    do not manage the cluster - run `just up` locally, or rely on workflow
    setup in CI."""
    try:
        resp = requests.get(f"{SERVER_URL}/health", timeout=2)
        if resp.status_code >= 500:
            pytest.fail(
                f"edgeflow server at {SERVER_URL} returned {resp.status_code}; "
                "start the stack with `just up`"
            )
    except requests.RequestException as exc:
        pytest.fail(
            f"edgeflow server at {SERVER_URL} is not reachable ({exc}); "
            "start the stack with `just up`"
        )


@pytest.fixture(scope="session")
def cli_path() -> Path:
    """Build edgeflow-cli once per session and return the binary path."""
    _log("building edgeflow-cli (cargo build -p edgeflow-cli)")
    subprocess.run(
        ["cargo", "build", "-p", "edgeflow-cli"],
        cwd=REPO_ROOT,
        check=True,
    )
    binary = REPO_ROOT / "target" / "debug" / "edgeflow"
    if not binary.exists():
        raise RuntimeError(f"cargo build succeeded but {binary} is missing")
    return binary


@pytest.fixture(scope="session")
def cli_json(cli_path: Path):
    """Factory: invoke `edgeflow --json <args...>` and return parsed JSON.

    Always sets --server explicitly so we never accidentally hit a different
    server set in the developer's shell environment.
    """

    def _invoke(*args: str) -> object:
        result = subprocess.run(
            [str(cli_path), "--server", SERVER_URL, "--json", *args],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise AssertionError(
                f"cli {' '.join(args)} exited {result.returncode}\n"
                f"stdout: {result.stdout}\nstderr: {result.stderr}"
            )
        if not result.stdout.strip():
            return None
        return json.loads(result.stdout)

    return _invoke


@pytest.fixture(scope="session")
def kubectl():
    """Factory: invoke `kubectl <args...>`, return stdout. Tests use this to
    assert cluster-state side effects of mutating commands."""

    def _invoke(*args: str, check: bool = True) -> str:
        result = subprocess.run(
            ["kubectl", *args],
            capture_output=True,
            text=True,
            check=False,
        )
        if check and result.returncode != 0:
            raise AssertionError(
                f"kubectl {' '.join(args)} exited {result.returncode}\n"
                f"stdout: {result.stdout}\nstderr: {result.stderr}"
            )
        return result.stdout

    return _invoke


@pytest.fixture
def unique_name(request: pytest.FixtureRequest):
    """Factory: returns names like `tgt-test-foo-a1b2c3d4` so concurrent
    tests / reruns can't collide on cluster-wide names."""
    suffix = uuid.uuid4().hex[:8]
    test_slug = re.sub(r"[^a-z0-9-]+", "-", request.node.name.lower()).strip("-")[:30]

    def _gen(prefix: str) -> str:
        return f"{prefix}-{test_slug}-{suffix}"

    return _gen


@pytest.fixture
def cleanup_targets():
    """Track targets created during a test; tear them all down afterwards.
    Tests register names via `cleanup_targets.add(name)`; teardown calls the
    SDK regardless of test outcome so leaked state can't poison later tests."""
    from edgeflow import targets as sdk_targets  # local import: SDK is a dev-dep

    created: list[str] = []

    class _Tracker:
        def add(self, name: str) -> None:
            created.append(name)

    yield _Tracker()

    for name in created:
        try:
            sdk_targets.teardown(name, server=SERVER_URL)
        except Exception as exc:
            _log(f"cleanup_targets: teardown of '{name}' failed: {exc}")
