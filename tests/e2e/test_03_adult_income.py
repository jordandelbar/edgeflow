"""E2E test for tutorial 03 (docs/book/tutorials/03-adult-income.rst).

Validates named-input JSON mode: client sends a dict with categorical and
numeric fields, server applies the column-transformer encodings from
schema.json, model returns a labelled prediction.

Loose assertion on the label - F1 is ~0.71 so we don't bet the test on a
specific class for any one input. We just verify the response shape is what
the tutorial promises.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import requests

TUTORIAL_DIR_NAME = "examples/03-adult-income"
TRAIN_TIMEOUT_SECONDS = 240


def test_adult_income_train_and_infer(
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

    sample = {
        "workclass": "Private",
        "education": "Bachelors",
        "marital-status": "Married-civ-spouse",
        "occupation": "Exec-managerial",
        "relationship": "Husband",
        "race": "White",
        "sex": "Male",
        "native-country": "United-States",
        "age": 45,
        "fnlwgt": 200000,
        "education-num": 13,
        "capital-gain": 0,
        "capital-loss": 0,
        "hours-per-week": 40,
    }
    response = requests.post(
        f"{inference_url}/infer",
        json=sample,
        timeout=10,
    )
    response.raise_for_status()
    payload = response.json()

    assert payload["label"] in {">50K", "<=50K"}
    assert 0.0 <= payload["confidence"] <= 1.0
