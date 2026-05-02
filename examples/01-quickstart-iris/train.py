# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "edgeflow",
#   "mlflow>=3.11.1,<4",
#   "numpy>=2.4.4,<3",
#   "scikit-learn>=1.8.0,<2",
# ]
# ///
# Tutorial: https://github.com/jordandelbar/edgeflow/blob/main/docs/book/tutorials/01-quickstart-iris.rst
"""
Iris LogisticRegression - train, register, deploy.

Run standalone with no venv setup:
  uv run train.py

After it finishes:
  curl -X POST http://localhost:8080/infer \\
       -H 'Content-Type: application/json' \\
       -d '[5.1, 3.5, 1.4, 0.2]'
"""

# [docs:start:imports]
import os

import edgeflow
import mlflow
import numpy as np
from edgeflow.models import sklearn_to_onnx
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score
from sklearn.model_selection import train_test_split
# [docs:end:imports]

# config

# [docs:start:config]
EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "quickstart")
# [docs:end:config]

# train

print("training iris classifier...")
# [docs:start:train]
iris = load_iris()
X_train, X_test, y_train, y_test = train_test_split(
    iris.data.astype(np.float32), iris.target, test_size=0.2, random_state=42
)
clf = LogisticRegression(max_iter=200)
clf.fit(X_train, y_train)
accuracy = accuracy_score(y_test, clf.predict(X_test))
# [docs:end:train]
print(f"accuracy: {accuracy:.4f}")

# push to edgeflow

print(f"pushing to edgeflow at {EDGEFLOW_SERVER}...")
# [docs:start:mlflow-run]
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
# [docs:end:mlflow-run]

print(f"run_id: {run_id}")

# register + deploy

# [docs:start:register-deploy]
mv = edgeflow.register(run_id, "iris-classifier", server=EDGEFLOW_SERVER)
deployment = edgeflow.deploy(
    mv.name, mv.version, EDGEFLOW_TARGET, server=EDGEFLOW_SERVER, wait=True
)
# [docs:end:register-deploy]
