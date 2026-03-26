"""
Helpers for logging edgeflow artifacts to an active mlflow run.
"""
from __future__ import annotations

import importlib.resources
import tempfile
import warnings
from pathlib import Path


def log_model(
    model_bytes: bytes,
    preprocess=None,
    postprocess=None,
    wit_dir: Path | None = None,
) -> None:
    """Log an ONNX model and transforms to the active mlflow run.

    Standard path — Pipeline objects (recommended):
        Uses the pre-compiled Rust standard_pipeline.wasm (~150 KB).
        No extra tools required; no Rust compiler on the user's machine.

    Legacy path — @preprocess / @postprocess decorators:
        Falls back to componentize-py (requires wit_dir). Produces ~40 MB
        WASM components and ~800 MB inference memory. Emits a UserWarning.

    Must be called inside an active mlflow.start_run() context.

    Args:
        model_bytes:  serialised ONNX model bytes (e.g. from clf_to_onnx()).
        preprocess:   Pipeline for input transforms, or None.
        postprocess:  Pipeline for output transforms, or None.
        wit_dir:      path to WIT definitions — only required for legacy path.
    """
    import mlflow
    from edgeflow.pipeline import Pipeline

    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = Path(tmp)
        model_path = tmpdir / "model.onnx"
        model_path.write_bytes(model_bytes)
        mlflow.log_artifact(str(model_path))

        if isinstance(preprocess, Pipeline) or isinstance(postprocess, Pipeline):
            _log_standard(tmpdir, preprocess, postprocess)
        else:
            _log_legacy(tmpdir, wit_dir)


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
