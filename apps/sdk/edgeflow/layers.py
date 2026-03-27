"""
Standard transform layers for the edgeflow inference pipeline.

Each layer serialises to a JSON config understood by the Rust
standard_pipeline.wasm component shipped in this package.  Calling
Pipeline([...]).transform() locally runs the exact same Rust logic via
the native PyO3 extension — server results are guaranteed to match.
"""
from __future__ import annotations

from dataclasses import dataclass


class Layer:
    def to_config(self) -> dict:
        raise NotImplementedError


@dataclass
class FloatBytesToTensor(Layer):
    """Convert raw f32 LE bytes to tensor wire format.

    Wraps n_features raw floats from the request body into the edgeflow
    tensor wire format expected by the inference backend.

    Args:
        n_features: number of f32 values expected in the input bytes.
    """
    n_features: int

    def to_config(self) -> dict:
        return {"type": "float_to_tensor", "n_features": self.n_features}


@dataclass
class Normalize(Layer):
    """Per-feature z-score normalisation applied to a tensor.

    Requires a tensor in wire format as input; outputs a normalised tensor.
    Typically chained after FloatBytesToTensor.

    Args:
        mean: per-feature mean values.
        std:  per-feature standard deviation values.
    """
    mean: list[float]
    std: list[float]

    def to_config(self) -> dict:
        return {"type": "normalize", "mean": self.mean, "std": self.std}


@dataclass
class ClassifierOutput(Layer):
    """Argmax over a probability tensor; returns JSON result bytes.

    Produces: {"class_id": <int>, "label": "<str>", "confidence": <float>}

    Args:
        labels: ordered list of class label strings.
    """
    labels: list[str]

    def to_config(self) -> dict:
        return {"type": "classifier_output", "labels": self.labels}


@dataclass
class RawTensorOutput(Layer):
    """Pass tensor wire format bytes through unchanged."""

    def to_config(self) -> dict:
        return {"type": "raw_tensor_output"}
