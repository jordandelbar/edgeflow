"""
Standard transform layers for the edgeflow inference pipeline.

Each layer serialises to a JSON config understood by the Rust
standard_pipeline.wasm component shipped in this package.  Calling
Pipeline([...]).transform() locally runs the exact same Rust logic via
the native PyO3 extension - server results are guaranteed to match.
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


@dataclass
class ImageToTensor(Layer):
    """Decode raw JPEG/PNG bytes, resize, normalise to [0, 1], and reorder HWC → CHW.

    Output tensor shape: [1, 3, height, width].  Use as the first preprocess step
    for image-based models (e.g. YOLO, DETR).  The server accepts the raw image
    bytes directly when this layer is the first step; set Content-Type to
    image/jpeg or image/png on the inference request.

    Args:
        width:  target image width in pixels (e.g. 640 for YOLOv8).
        height: target image height in pixels (e.g. 640 for YOLOv8).
    """

    width: int
    height: int

    def to_config(self) -> dict:
        return {"type": "image_to_tensor", "width": self.width, "height": self.height}


@dataclass
class DetectionOutput(Layer):
    """Decode a YOLO-style detection tensor and apply NMS; returns a JSON array.

    Expects input tensor shape [1, 4+num_classes, num_boxes] - the standard
    YOLOv8 ONNX export format.  Bounding box coordinates in the output are
    normalised to [0, 1]; multiply by your display dimensions to get pixel coords.

    Produces:
        [{"class_id": int, "label": str, "confidence": float,
          "bbox": [x1, y1, x2, y2]}, ...]

    Args:
        labels:           ordered list of class label strings (must match model).
        conf_threshold:   discard detections below this confidence (default 0.5).
        iou_threshold:    IoU threshold for greedy NMS (default 0.7).
        model_size:       square input size the model was trained on (default 640).
    """

    labels: list[str]
    conf_threshold: float = 0.5
    iou_threshold: float = 0.7
    model_size: int = 640

    def to_config(self) -> dict:
        return {
            "type": "detection_output",
            "labels": self.labels,
            "conf_threshold": self.conf_threshold,
            "iou_threshold": self.iou_threshold,
            "model_size": self.model_size,
        }
