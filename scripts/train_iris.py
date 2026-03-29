"""
Iris LogisticRegression training script.

Flow:
  1. Train sklearn LogisticRegression on iris
  2. Export to ONNX via edgeflow.models.sklearn_to_onnx
  3. Push model + standard transforms to edgeflow
  4. Trigger deployment

Input protocol: 16 raw bytes — 4 × f32 LE (sepal_len, sepal_w, petal_len, petal_w)

  python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \\
    | curl -s -X POST http://localhost:8080/infer --data-binary @-
"""

import os

import edgeflow
import mlflow
import numpy as np
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score
from sklearn.model_selection import train_test_split

from edgeflow.models import sklearn_to_onnx

# ── config ─────────────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "iris-inference")

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

# ── push to edgeflow ───────────────────────────────────────────────────────────

print(f"pushing to edgeflow at {EDGEFLOW_SERVER}...")
mlflow.set_tracking_uri(EDGEFLOW_SERVER)
exp = mlflow.set_experiment("iris-poc")

with mlflow.start_run(experiment_id=exp.experiment_id, run_name="iris-logistic") as run:
    mlflow.log_params(
        {
            "model": "LogisticRegression",
            "max_iter": 200,
            "n_features": 4,
            "n_classes": 3,
        }
    )
    mlflow.log_metric("accuracy", accuracy)
    edgeflow.log_model(
        model_bytes=sklearn_to_onnx(clf),
        postprocess=edgeflow.Pipeline(
            [edgeflow.ClassifierOutput(labels=list(iris.target_names))]
        ),
    )
    run_id = run.info.run_id

print(f"run_id: {run_id}")

# ── register + deploy ──────────────────────────────────────────────────────────

mv = edgeflow.register(run_id, "iris-classifier", server=EDGEFLOW_SERVER)
deployment = edgeflow.deploy(
    mv.name, mv.version, EDGEFLOW_TARGET, server=EDGEFLOW_SERVER, wait=False
)
