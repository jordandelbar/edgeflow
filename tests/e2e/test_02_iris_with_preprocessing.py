"""E2E test for tutorial 02 (docs/book/tutorials/02-iris-with-preprocessing.rst).

The deployed pipeline has a WASM Normalize pre-transform that z-scores the
input server-side. The client sends the same JSON array as tutorial 01 -
the format-adapter step is skipped via `transform-from` and the array
goes straight into Normalize. Same setosa prediction shape; the difference
is invisible at the wire level, which is the whole point of the tutorial.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import requests

TUTORIAL_DIR_NAME = "examples/02-iris-with-preprocessing"
TRAIN_TIMEOUT_SECONDS = 90


def test_iris_with_preprocessing_train_and_infer(
    edgeflow_stack,
    inference_url,
    repo_root: Path,
    local_sdk_wheel: Path,
):
    tutorial_dir = repo_root / TUTORIAL_DIR_NAME

    env = {**os.environ, "PYTHONUNBUFFERED": "1"}
    subprocess.run(
        [
            "uv",
            "run",
            "--find-links",
            str(local_sdk_wheel.parent),
            "--reinstall-package",
            "edgeflow",
            "train.py",
        ],
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
