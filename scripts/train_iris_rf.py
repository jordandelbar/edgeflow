"""
Iris RandomForest training script.

Same pipeline as train_iris.py but uses RandomForestClassifier exported
via skl2onnx (requires ORT inference backend).

Input protocol: 16 raw bytes — 4 × f32 LE (sepal_len, sepal_w, petal_len, petal_w)

  python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \\
    | curl -s -X POST http://localhost:8080/infer --data-binary @-
"""

import os
import tempfile
from pathlib import Path

import mlflow
import numpy as np
import requests
from sklearn.datasets import load_iris
from sklearn.ensemble import RandomForestClassifier
from sklearn.metrics import accuracy_score
from sklearn.model_selection import train_test_split

from edgeflow.models import clf_to_onnx
from edgeflow.transforms import compile_transforms

import transforms  # noqa: F401  — registers @preprocess / @postprocess

# ── config ─────────────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "iris-inference")

N_ESTIMATORS = 100
MAX_DEPTH = 5

ROOT = Path(__file__).parent.parent
WIT_DIR = ROOT / "wit"

# ── train ──────────────────────────────────────────────────────────────────────

print("training iris random forest...")
iris = load_iris()
X_train, X_test, y_train, y_test = train_test_split(
    iris.data.astype(np.float32), iris.target, test_size=0.2, random_state=42
)
clf = RandomForestClassifier(n_estimators=N_ESTIMATORS, max_depth=MAX_DEPTH, random_state=42)
clf.fit(X_train, y_train)
accuracy = accuracy_score(y_test, clf.predict(X_test))
print(f"accuracy: {accuracy:.4f}")

# ── export + compile ───────────────────────────────────────────────────────────

with tempfile.TemporaryDirectory() as _tmp:
    tmpdir = Path(_tmp)

    print("exporting to ONNX via skl2onnx...")
    model_path = tmpdir / "model.onnx"
    model_path.write_bytes(clf_to_onnx(clf))

    print("compiling transforms to WASM components via componentize-py...")
    wasm_artifacts = compile_transforms(wit_dir=WIT_DIR, output_dir=tmpdir)

    # ── push to edgeflow ───────────────────────────────────────────────────────

    print(f"pushing to edgeflow at {EDGEFLOW_SERVER}...")
    mlflow.set_tracking_uri(EDGEFLOW_SERVER)
    exp = mlflow.set_experiment("iris-poc")

    with mlflow.start_run(experiment_id=exp.experiment_id, run_name="iris-random-forest") as run:
        mlflow.log_params({
            "model": "RandomForestClassifier",
            "n_estimators": N_ESTIMATORS,
            "max_depth": MAX_DEPTH,
            "n_features": 4,
            "n_classes": 3,
        })
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
