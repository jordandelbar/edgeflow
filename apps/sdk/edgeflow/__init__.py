"""edgeflow Python SDK.

Two layers:

- **Training-time helpers**: ``log_model``, ``Pipeline``, the layer classes
  (``ClassifierOutput``, ``DetectionOutput``, ``Normalize`` ...), and the
  ``clf_to_onnx`` exporter family in :mod:`edgeflow.models`.
- **REST ops**: subpackages mirroring the ``edgeflow`` CLI subcommands -
  :mod:`edgeflow.targets`, :mod:`edgeflow.deployments`,
  :mod:`edgeflow.models`, :mod:`edgeflow.runs`,
  :mod:`edgeflow.experiments`, :mod:`edgeflow.nodes`. Plus top-level
  ``register`` and ``deploy`` for the common training-script flow.

The CLI and SDK share one Rust HTTP client (``edgeflow._lib.Client``);
calls go through the same retry/error path on both surfaces.
"""

from edgeflow import deployments, experiments, models, nodes, runs, targets
from edgeflow._types import (
    Experiment,
    InfraSettings,
    RegisteredModel,
    ResourceSettings,
    Run,
    Target,
    TargetPod,
)
from edgeflow.artifacts import log_model
from edgeflow.deploy import Deployment, ModelVersion, deploy, register
from edgeflow.layers import (
    ClassifierOutput,
    DetectionOutput,
    FloatBytesToTensor,
    ImageToTensor,
    Normalize,
    RawTensorOutput,
)
from edgeflow.pipeline import Pipeline

__all__ = [
    # Training-time
    "log_model",
    "Pipeline",
    "FloatBytesToTensor",
    "Normalize",
    "ClassifierOutput",
    "RawTensorOutput",
    "ImageToTensor",
    "DetectionOutput",
    # Top-level ops (kept for backward-compat with existing examples)
    "register",
    "deploy",
    # REST ops subpackages (mirror CLI subcommands)
    "targets",
    "deployments",
    "models",
    "runs",
    "experiments",
    "nodes",
    # Typed return shapes
    "ModelVersion",
    "Deployment",
    "Target",
    "TargetPod",
    "ResourceSettings",
    "InfraSettings",
    "Run",
    "Experiment",
    "RegisteredModel",
]
