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
TRAIN_TIMEOUT_SECONDS = 90


def test_quickstart_iris_train_and_infer(
    edgeflow_stack,
    inference_url,
    repo_root: Path,
    tutorial_python: Path,
):
    tutorial_dir = repo_root / TUTORIAL_DIR_NAME

    env = {**os.environ, "PYTHONUNBUFFERED": "1"}
    subprocess.run(
        [str(tutorial_python), "train.py"],
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
