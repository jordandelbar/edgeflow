"""
Helpers for logging edgeflow artifacts to an active mlflow run.
"""
from __future__ import annotations

import importlib.resources
import json
import tempfile
import warnings
from pathlib import Path


def log_model(
    model_bytes: bytes,
    preprocess=None,
    postprocess=None,
    wit_dir: Path | None = None,
    column_transformer=None,
) -> None:
    """Log an ONNX model and transforms to the active mlflow run.

    Standard path — Pipeline objects (recommended):
        Uses the pre-compiled Rust standard_pipeline.wasm (~150 KB).
        No extra tools required; no Rust compiler on the user's machine.

        FloatBytesToTensor is injected automatically when the ONNX model
        has a single float[batch, n_features] input and the preprocess
        pipeline does not already start with one.  In most cases you only
        need to supply postprocess.

    Named-input path — mixed tabular models:
        Pass ``column_transformer`` (a fitted sklearn ColumnTransformer) to
        enable JSON input mode.  Encoding tables are extracted from the
        fitted transformer and written to schema.json.  The inference server
        will accept a JSON body, apply the encodings, and feed the resulting
        float tensor to the ONNX model.  The ONNX model must be exported
        post-encoding (i.e. the classifier only, not the full sklearn pipeline).

    Legacy path — @preprocess / @postprocess decorators:
        Falls back to componentize-py (requires wit_dir). Produces ~40 MB
        WASM components and ~800 MB inference memory. Emits a UserWarning.

    Must be called inside an active mlflow.start_run() context.

    Args:
        model_bytes:         serialised ONNX model bytes (e.g. from clf_to_onnx()).
        preprocess:          Pipeline for input transforms, or None.
        postprocess:         Pipeline for output transforms, or None.
        wit_dir:             path to WIT definitions — only required for legacy path.
        column_transformer:  fitted sklearn ColumnTransformer.  When supplied,
                             Named-input mode is activated and FloatBytesToTensor
                             auto-injection is skipped.
    """
    import mlflow
    from edgeflow.pipeline import Pipeline
    from edgeflow.layers import FloatBytesToTensor, ImageToTensor

    if column_transformer is None:
        # Single-tensor path: auto-inject FloatBytesToTensor from ONNX input shape.
        # Skip injection when the first step is already ImageToTensor — image
        # models produce their own tensor inside the preprocess WASM.
        n = _read_onnx_n_features(model_bytes)
        if n is not None:
            first = preprocess.steps[0] if (isinstance(preprocess, Pipeline) and preprocess.steps) else None
            if preprocess is None:
                preprocess = Pipeline([FloatBytesToTensor(n_features=n)])
            elif isinstance(preprocess, Pipeline) and not isinstance(first, (FloatBytesToTensor, ImageToTensor)):
                preprocess = Pipeline([FloatBytesToTensor(n_features=n)] + preprocess.steps)

    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = Path(tmp)
        model_path = tmpdir / "model.onnx"
        model_path.write_bytes(model_bytes)
        mlflow.log_artifact(str(model_path))

        if isinstance(preprocess, Pipeline) or isinstance(postprocess, Pipeline):
            _log_standard(tmpdir, preprocess, postprocess)
        elif column_transformer is None:
            _log_legacy(tmpdir, wit_dir)

        schema_path = tmpdir / "schema.json"
        schema = _build_schema(preprocess, postprocess, column_transformer)
        schema_path.write_bytes(json.dumps(schema).encode())
        mlflow.log_artifact(str(schema_path))


def _log_standard(tmpdir: Path, preprocess, postprocess) -> None:
    import mlflow

    wasm_bytes = (
        importlib.resources.files("edgeflow")
        .joinpath("wasm/standard_pipeline.wasm")
        .read_bytes()
    )
    for role, pipeline in (("preprocess", preprocess), ("postprocess", postprocess)):
        if pipeline is None:
            continue
        (tmpdir / f"{role}.wasm").write_bytes(wasm_bytes)
        (tmpdir / f"{role}.json").write_bytes(pipeline.to_config())
        mlflow.log_artifact(str(tmpdir / f"{role}.wasm"))
        mlflow.log_artifact(str(tmpdir / f"{role}.json"))


def _read_onnx_n_features(model_bytes: bytes) -> int | None:
    """Return n_features from a float[batch, n_features] ONNX input, or None."""
    try:
        from edgeflow.models import read_onnx_input_shape
        return read_onnx_input_shape(model_bytes)
    except Exception:
        return None


def _build_schema(preprocess, postprocess, column_transformer=None) -> dict:
    from edgeflow.layers import FloatBytesToTensor, ClassifierOutput, ImageToTensor, DetectionOutput

    schema: dict = {}

    if column_transformer is not None:
        # Named-input mode: encoding tables extracted from the fitted transformer.
        fields = _build_field_specs_from_transformer(column_transformer)
        schema["input"] = {"format": "json", "fields": fields}
    elif preprocess is not None and preprocess.steps:
        first = preprocess.steps[0]
        if isinstance(first, FloatBytesToTensor):
            schema["input"] = {"format": "float_bytes", "n_features": first.n_features}
        elif isinstance(first, ImageToTensor):
            schema["input"] = {"format": "image", "width": first.width, "height": first.height}

    if postprocess is not None and postprocess.steps:
        last = postprocess.steps[-1]
        if isinstance(last, ClassifierOutput):
            schema["output"] = {"format": "json", "labels": last.labels}
        elif isinstance(last, DetectionOutput):
            schema["output"] = {
                "format": "json",
                "labels": last.labels,
                "conf_threshold": last.conf_threshold,
                "iou_threshold": last.iou_threshold,
            }
        else:
            schema["output"] = {"format": "tensor"}
    else:
        schema["output"] = {"format": "tensor"}

    return schema


def _build_field_specs_from_transformer(column_transformer) -> list[dict]:
    """Extract per-field encoding specs from a fitted sklearn ColumnTransformer.

    Fields are emitted in the same order as ``column_transformer.transform()``
    output — this order must match what the deployed ONNX model was trained on.

    Supported transformers:
        OrdinalEncoder  → ``{"type": "ordinal", "map": {category: index}}``
        OneHotEncoder   → ``{"type": "one_hot", "categories": [...]}``
        "passthrough"   → ``{"type": "float"}`` (no encoding entry)
    """
    from sklearn.preprocessing import OrdinalEncoder, OneHotEncoder
    from sklearn.preprocessing import FunctionTransformer

    fields = []
    for _step_name, transformer, columns in column_transformer.transformers_:
        if _step_name == "remainder":
            continue

        cols = list(columns) if not isinstance(columns, list) else columns

        for col_idx, col in enumerate(cols):
            if transformer == "passthrough" or isinstance(transformer, FunctionTransformer):
                fields.append({"name": str(col), "type": "float"})

            elif isinstance(transformer, OrdinalEncoder):
                categories = transformer.categories_[col_idx]
                fields.append({
                    "name": str(col),
                    "type": "string",
                    "encoding": {
                        "type": "ordinal",
                        "map": {str(cat): float(i) for i, cat in enumerate(categories)},
                    },
                })

            elif isinstance(transformer, OneHotEncoder):
                categories = transformer.categories_[col_idx]
                fields.append({
                    "name": str(col),
                    "type": "string",
                    "encoding": {
                        "type": "one_hot",
                        "categories": [str(c) for c in categories],
                    },
                })

    return fields


def _log_legacy(tmpdir: Path, wit_dir: Path | None) -> None:
    import mlflow
    from edgeflow.transforms import compile_transforms

    if wit_dir is None:
        raise ValueError("wit_dir is required when using @preprocess/@postprocess transforms")

    warnings.warn(
        "Using componentize-py transforms (~40 MB WASM, ~800 MB inference memory). "
        "Consider migrating to Pipeline([...]) with standard layers.",
        UserWarning,
        stacklevel=3,
    )
    wasm = compile_transforms(wit_dir=wit_dir, output_dir=tmpdir)
    mlflow.log_artifact(str(wasm["preprocess"]))
    mlflow.log_artifact(str(wasm["postprocess"]))
