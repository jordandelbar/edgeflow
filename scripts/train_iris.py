"""
Iris PoC training script.

Flow:
  1. Train sklearn LogisticRegression on iris
  2. Export to ONNX via edgeflow.models.sklearn_to_onnx
  3. Import transforms.py (registers @preprocess / @postprocess via decorators)
  4. Compile transforms to WASM components via componentize-py
  5. Push to edgeflow: experiment, run, metrics, artifacts
  6. Trigger deployment via POST /api/v1/deployments

Input protocol (what POST /infer expects):
  16 raw bytes — 4 × f32 little-endian (sepal_len, sepal_w, petal_len, petal_w)

  python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \\
    | curl -s -X POST http://localhost:8080/infer --data-binary @-
"""

import os
import sys
import tempfile
from pathlib import Path

import mlflow
import numpy as np
import requests
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score
from sklearn.model_selection import train_test_split

# Add scripts/ to path so `from edgeflow...` resolves locally.
sys.path.insert(0, str(Path(__file__).parent))

from edgeflow.models import sklearn_to_onnx
from edgeflow.transforms import compile_transforms

# Importing transforms.py runs the decorators, registering prepare and interpret.
import transforms  # noqa: F401

# ── config ─────────────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "iris-inference")

ROOT = Path(__file__).parent.parent
WIT_DIR = ROOT / "wit"

# ── train ──────────────────────────────────────────────────────────────────────

print("training iris classifier...")
iris = load_iris()
X_train, X_test, y_train, y_test = train_test_split(
    iris.data.astype(np.float32), iris.target, test_size=0.2, random_state=42
)
clf = LogisticRegression(max_iter=200)
clf.fit(X_train, y_train)
accuracy = accuracy_score(y_test, clf.predict(X_test))
print(f"accuracy: {accuracy:.4f}")

# ── export + compile ───────────────────────────────────────────────────────────

with tempfile.TemporaryDirectory() as _tmp:
    tmpdir = Path(_tmp)

    print("exporting to ONNX...")
    model_path = tmpdir / "model.onnx"
    model_path.write_bytes(sklearn_to_onnx(clf))

    print("compiling transforms to WASM components via componentize-py...")
    wasm_artifacts = compile_transforms(wit_dir=WIT_DIR, output_dir=tmpdir)

    # ── push to edgeflow ───────────────────────────────────────────────────────

    print(f"pushing to edgeflow at {EDGEFLOW_SERVER}...")
    mlflow.set_tracking_uri(EDGEFLOW_SERVER)
    exp = mlflow.set_experiment("iris-poc")

    with mlflow.start_run(experiment_id=exp.experiment_id, run_name="iris-logistic") as run:
        mlflow.log_params(
            {"model": "LogisticRegression", "max_iter": 200, "n_features": 4, "n_classes": 3}
        )
        mlflow.log_metric("accuracy", accuracy)
        mlflow.log_artifact(str(model_path))
        mlflow.log_artifact(str(wasm_artifacts["preprocess"]))
        mlflow.log_artifact(str(wasm_artifacts["postprocess"]))
        run_id = run.info.run_id

    print(f"run_id: {run_id}")

    # ── trigger deployment ─────────────────────────────────────────────────────

    print(f"triggering deployment → target={EDGEFLOW_TARGET}...")
    resp = requests.post(
        f"{EDGEFLOW_SERVER}/api/v1/deployments",
        json={"run_id": run_id, "target": EDGEFLOW_TARGET},
    )
    resp.raise_for_status()
    deployment = resp.json()["deployment"]
    print(f"deployment_id: {deployment['deployment_id']}")

print()
print("done. to test inference:")
print(
    '  python3 -c "import struct, sys; sys.stdout.buffer.write('
    "struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))\" \\"
)
print("  | curl -s -X POST http://localhost:8080/infer --data-binary @-")
