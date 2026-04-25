"""E2E test for tutorial 01 (docs/book/tutorials/01-quickstart-iris.rst).

Mirrors the exact commands the tutorial tells the reader to run, then asserts
the inference response matches the documented shape. Assertions are loose on
purpose: the tutorial demonstrates behavior, not numerical reproducibility.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import requests

TUTORIAL_DIR_NAME = "examples/01-quickstart-iris"
# Iris LogReg trains in <1s. The slow part is deploy(wait=True) waiting for
# MQTT ack from the inference pod. If it doesn't ack in 90s, something's wrong.
TRAIN_TIMEOUT_SECONDS = 90


def test_quickstart_iris_train_and_infer(
    edgeflow_stack,
    inference_url,
    repo_root: Path,
    local_sdk_wheel: Path,
):
    tutorial_dir = repo_root / TUTORIAL_DIR_NAME

    # PYTHONUNBUFFERED forces line-buffered stdout/stderr so we still see the
    # script's progress output if it hangs and we have to SIGKILL it on timeout.
    env = {**os.environ, "PYTHONUNBUFFERED": "1"}
    # `--with <wheel>` overrides the script's inline `edgeflow` dep so we test
    # the current tree's SDK, not the last PyPI release.
    subprocess.run(
        ["uv", "run", "--with", str(local_sdk_wheel), "train.py"],
        cwd=tutorial_dir,
        check=True,
        timeout=TRAIN_TIMEOUT_SECONDS,
        env=env,
    )

    response = requests.post(
        f"{inference_url}/infer",
        headers={"Content-Type": "application/json"},
        json=[5.1, 3.5, 1.4, 0.2],
        timeout=10,
    )
    response.raise_for_status()
    payload = response.json()

    assert payload["class_id"] == 0
    assert payload["label"] == "setosa"
    assert payload["confidence"] > 0.9
