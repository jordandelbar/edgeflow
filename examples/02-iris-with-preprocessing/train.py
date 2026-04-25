# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "edgeflow",
#   "mlflow",
#   "scikit-learn",
# ]
# ///
# Tutorial: https://github.com/jordandelbar/edgeflow/blob/main/docs/book/tutorials/02-iris-with-preprocessing.rst
"""
Iris LogisticRegression trained on z-scored features.

The normalization is baked into the preprocess pipeline as a WASM Normalize
step. The caller sends the same JSON array as tutorial 01 - the WASM
transform handles the z-score server-side before the tensor reaches the
model.

This validates that preprocessing logic changes are absorbed by the artifact
with no changes to the inference server or the wire format.
"""

import os

import edgeflow
import mlflow
import numpy as np
from edgeflow.models import sklearn_to_onnx
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score
from sklearn.model_selection import train_test_split

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "iris-inference")

# ── train on normalised features ───────────────────────────────────────────────

iris = load_iris()
X = iris.data.astype(np.float32)
X_train, X_test, y_train, y_test = train_test_split(
    X, iris.target, test_size=0.2, random_state=42
)

mean = X_train.mean(axis=0).tolist()
std = X_train.std(axis=0).tolist()
print(f"feature mean: {[round(m, 4) for m in mean]}")
print(f"feature std:  {[round(s, 4) for s in std]}")

X_train_norm = (X_train - np.array(mean)) / np.array(std)
X_test_norm = (X_test - np.array(mean)) / np.array(std)

print("training on z-scored features...")
clf = LogisticRegression(max_iter=200)
clf.fit(X_train_norm, y_train)
accuracy = accuracy_score(y_test, clf.predict(X_test_norm))
print(f"accuracy: {accuracy:.4f}")

# ── push: normalization is inside the preprocess pipeline ─────────────────────

print(f"pushing to edgeflow at {EDGEFLOW_SERVER}...")
mlflow.set_tracking_uri(EDGEFLOW_SERVER)
exp = mlflow.set_experiment("iris-poc")

with mlflow.start_run(
    experiment_id=exp.experiment_id, run_name="iris-normalized"
) as run:
    mlflow.log_params(
        {
            "model": "LogisticRegression",
            "preprocessing": "z-score",
            "mean": mean,
            "std": std,
        }
    )
    mlflow.log_metric("accuracy", accuracy)
    edgeflow.log_model(
        model_bytes=sklearn_to_onnx(clf),
        preprocess=edgeflow.Pipeline(
            [
                edgeflow.Normalize(mean=mean, std=std),
            ]
        ),
        postprocess=edgeflow.Pipeline(
            [
                edgeflow.ClassifierOutput(labels=list(iris.target_names)),
            ]
        ),
    )
    run_id = run.info.run_id

print(f"run_id: {run_id}")

# ── register + deploy ──────────────────────────────────────────────────────────

mv = edgeflow.register(run_id, "iris-classifier", server=EDGEFLOW_SERVER)
deployment = edgeflow.deploy(
    mv.name, mv.version, EDGEFLOW_TARGET, server=EDGEFLOW_SERVER, wait=True
)
