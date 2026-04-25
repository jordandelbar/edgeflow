# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "edgeflow[xgboost]",
#   "mlflow",
#   "scikit-learn",
#   "xgboost",
#   "pandas",
# ]
# ///
# Tutorial: https://github.com/jordandelbar/edgeflow/blob/main/docs/book/tutorials/03-adult-income.rst
"""
Adult Income classifier training script.

Tests edgeflow's Named-input mode: the model has mixed categorical + numeric
features.  The sklearn ColumnTransformer is passed to edgeflow so its encoding
tables are written to schema.json.  The inference server accepts a JSON body,
applies the encodings, and feeds the resulting float tensor to the ONNX model.

Dataset: UCI Adult Income, pulled as CSV.
Model:   XGBClassifier (default) | LGBMClassifier | CatBoostClassifier
         Set EDGEFLOW_MODEL_TYPE=xgboost|lightgbm|catboost (default: xgboost)
Target:  binary - '>50K' income or not

Input protocol (Named mode): JSON body, e.g.
  curl -s -X POST http://localhost:8080/infer \\
    -H "content-type: application/json" \\
    -d '{
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
      "hours-per-week": 40
    }'
"""

import os

import edgeflow
import mlflow
import numpy as np
import pandas as pd
from edgeflow.models import clf_to_onnx
from sklearn.compose import ColumnTransformer
from sklearn.metrics import f1_score, roc_auc_score
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import OrdinalEncoder

# ── config ─────────────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "adult-inference")
MODEL_TYPE = os.environ.get("EDGEFLOW_MODEL_TYPE", "xgboost")

N_ESTIMATORS = 200
MAX_DEPTH = 4
LEARNING_RATE = 0.1
ADULT_URL = "https://archive.ics.uci.edu/ml/machine-learning-databases/adult/adult.data"
COLUMN_NAMES = [
    "age",
    "workclass",
    "fnlwgt",
    "education",
    "education-num",
    "marital-status",
    "occupation",
    "relationship",
    "race",
    "sex",
    "capital-gain",
    "capital-loss",
    "hours-per-week",
    "native-country",
    "target",
]
CATEGORICAL_COLS = [
    "workclass",
    "education",
    "marital-status",
    "occupation",
    "relationship",
    "race",
    "sex",
    "native-country",
]

# ── model factory ───────────────────────────────────────────────────────────────


def make_clf(model_type: str):
    """Return a fitted-ready classifier based on MODEL_TYPE."""
    if model_type == "xgboost":
        from xgboost import XGBClassifier

        return XGBClassifier(
            n_estimators=N_ESTIMATORS,
            max_depth=MAX_DEPTH,
            learning_rate=LEARNING_RATE,
            objective="binary:logistic",
            eval_metric="logloss",
            random_state=42,
        )
    if model_type == "lightgbm":
        from lightgbm import LGBMClassifier

        return LGBMClassifier(
            n_estimators=N_ESTIMATORS,
            max_depth=MAX_DEPTH,
            learning_rate=LEARNING_RATE,
            random_state=42,
            verbose=-1,
        )
    if model_type == "catboost":
        from catboost import CatBoostClassifier

        return CatBoostClassifier(
            iterations=N_ESTIMATORS,
            depth=MAX_DEPTH,
            learning_rate=LEARNING_RATE,
            random_seed=42,
            verbose=0,
        )
    raise ValueError(
        f"unknown EDGEFLOW_MODEL_TYPE: {model_type!r}. "
        "Use xgboost, lightgbm, or catboost."
    )


# ── dataset ────────────────────────────────────────────────────────────────────

print(f"model type: {MODEL_TYPE}")
print(f"fetching adult income dataset from {ADULT_URL}...")
df = pd.read_csv(
    ADULT_URL,
    names=COLUMN_NAMES,
    na_values="?",
    skipinitialspace=True,
)
y = (df["target"] == ">50K").astype(int)
X = df.drop(columns="target")
for col in CATEGORICAL_COLS:
    X[col] = X[col].astype("category")

print(f"dataset: {X.shape[0]:,} rows, {X.shape[1]} features")
print(f"class balance: {y.mean():.1%} >50K")

# ── preprocessing ──────────────────────────────────────────────────────────────

categorical_cols = [col for col in X.columns if X[col].dtype.name == "category"]
numerical_cols = [col for col in X.columns if col not in categorical_cols]

print(f"\ncategorical features ({len(categorical_cols)}): {categorical_cols}")
print(f"numerical features  ({len(numerical_cols)}): {numerical_cols}")

# OrdinalEncoder for all categoricals - no OHE expansion needed for tree models.
# unknown_value=-1 handles unseen categories at inference time gracefully.
# [docs:start:column-transformer]
preprocessor = ColumnTransformer(
    [
        (
            "cat",
            OrdinalEncoder(handle_unknown="use_encoded_value", unknown_value=-1),
            categorical_cols,
        ),
        ("num", "passthrough", numerical_cols),
    ]
)
# [docs:end:column-transformer]

# ── train ──────────────────────────────────────────────────────────────────────

X_train, X_test, y_train, y_test = train_test_split(
    X, y, test_size=0.2, random_state=42, stratify=y
)

X_train_enc = preprocessor.fit_transform(X_train).astype(np.float32)
X_test_enc = preprocessor.transform(X_test).astype(np.float32)

print(f"\nencoded feature count: {X_train_enc.shape[1]}")
print(f"training {MODEL_TYPE}...")

clf = make_clf(MODEL_TYPE)
clf.fit(X_train_enc, y_train)

y_pred = clf.predict(X_test_enc)
y_proba = clf.predict_proba(X_test_enc)[:, 1]
f1 = f1_score(y_test, y_pred)
auc = roc_auc_score(y_test, y_proba)
print(f"F1: {f1:.4f}  AUC-ROC: {auc:.4f}")

# ── push to edgeflow ───────────────────────────────────────────────────────────

print(f"\npushing to edgeflow at {EDGEFLOW_SERVER}...")
mlflow.set_tracking_uri(EDGEFLOW_SERVER)
exp = mlflow.set_experiment("adult-income-poc")

with mlflow.start_run(
    experiment_id=exp.experiment_id, run_name=f"adult-income-{MODEL_TYPE}"
) as run:
    mlflow.log_params(
        {
            "model": MODEL_TYPE,
            "n_estimators": N_ESTIMATORS,
            "max_depth": MAX_DEPTH,
            "learning_rate": LEARNING_RATE,
            "n_features_raw": X.shape[1],
            "n_features_encoded": X_train_enc.shape[1],
            "categorical_features": len(categorical_cols),
            "numerical_features": len(numerical_cols),
        }
    )
    mlflow.log_metric("f1_score", f1)
    mlflow.log_metric("roc_auc", auc)

    # Export the classifier only (post-encoding, single float tensor input).
    # clf_to_onnx detects the framework and uses the appropriate export path.
    # The column_transformer is passed separately so edgeflow can write the
    # encoding tables to schema.json - the server applies them at request time.
    # [docs:start:log-model]
    edgeflow.log_model(
        model_bytes=clf_to_onnx(clf),
        postprocess=edgeflow.Pipeline(
            [edgeflow.ClassifierOutput(labels=["<=50K", ">50K"])]
        ),
        column_transformer=preprocessor,
    )
    # [docs:end:log-model]
    run_id = run.info.run_id

print(f"run_id: {run_id}")

# ── register + deploy ──────────────────────────────────────────────────────────

mv = edgeflow.register(run_id, "adult-income-classifier", server=EDGEFLOW_SERVER)
deployment = edgeflow.deploy(
    mv.name, mv.version, EDGEFLOW_TARGET, server=EDGEFLOW_SERVER, wait=True
)
