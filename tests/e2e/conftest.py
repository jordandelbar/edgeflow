"""Shared fixtures for tutorial e2e tests.

Uses the dev compose file so we test the code in the current tree, not the
last published GHCR image. Set EDGEFLOW_E2E_SKIP_STACK=1 to point the tests
at a stack you started yourself (skip build + boot + teardown).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import time
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
    """Build the local SDK as a wheel so tutorial scripts run against the
    current tree's edgeflow, not the last published PyPI version."""
    transforms_dir = REPO_ROOT / "crates" / "transforms"
    sdk_dir = REPO_ROOT / "apps" / "sdk"

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

    wheels_dir = tmp_path_factory.mktemp("sdk-wheel")
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

    wheels = list(wheels_dir.glob("edgeflow-*.whl"))
    if len(wheels) != 1:
        raise RuntimeError(
            f"expected exactly 1 edgeflow wheel, found {len(wheels)} in {wheels_dir}"
        )
    _log(f"sdk wheel ready: {wheels[0].name}")
    return wheels[0]


@pytest.fixture(scope="session")
def tutorial_python(
    local_sdk_wheel: Path, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """Venv pre-installed with the local SDK wheel (with all extras) + every
    dep any tutorial needs. Tests run train scripts via this venv's python
    rather than `uv run`, because uv's resolution heuristics make it hard
    to force the local wheel over PyPI for ties (and `--with <wheel>` does
    not propagate PEP 723 [extras] declarations)."""
    venv = tmp_path_factory.mktemp("tutorial-venv")
    _log(f"creating tutorial venv at {venv}")
    subprocess.run(
        ["uv", "venv", str(venv), "--python", "3.13"],
        check=True,
    )
    py = venv / "bin" / "python"
    _log("installing local sdk wheel + tutorial deps into venv")
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            str(py),
            f"{local_sdk_wheel}[xgboost,lightgbm]",
            "mlflow",
            "scikit-learn",
            "numpy",
            "xgboost",
        ],
        check=True,
    )
    return py
