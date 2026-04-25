# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "edgeflow",
#   "mlflow",
#   "ultralytics",
# ]
# ///
# Tutorial: https://github.com/jordandelbar/edgeflow/blob/main/docs/book/tutorials/05-k3d-yolo.rst
"""
YOLOv8 deployment script.

Downloads a pretrained YOLOv8n checkpoint via ultralytics, exports it to ONNX,
and registers it with edgeflow using the ImageToTensor + DetectionOutput pipeline.
No training is performed - YOLO is used as-is from the COCO pretrained weights.

The inference server accepts raw JPEG or PNG bytes and returns a JSON array of
detected objects with normalised bounding boxes:

  curl -s -X POST http://localhost:8080/infer \\
    -H "content-type: image/jpeg" \\
    --data-binary @photo.jpg

  # response:
  [
    {"class_id": 0, "label": "person",  "confidence": 0.9134, "bbox": [0.23, 0.18, 0.67, 0.92]},
    {"class_id": 2, "label": "car",     "confidence": 0.8701, "bbox": [0.01, 0.42, 0.38, 0.81]},
    ...
  ]

Bounding box coordinates are normalised to [0, 1].  Multiply by your display
dimensions (width, height, width, height) to get pixel coordinates.

Model:   YOLOv8n (nano) - ~6 MB, 80 COCO classes
ONNX output shape: [1, 84, 8400]  (4 bbox coords + 80 class scores, 8400 anchors)
"""

import os
import tempfile
from pathlib import Path

import edgeflow
import mlflow

# ── config ─────────────────────────────────────────────────────────────────────

EDGEFLOW_SERVER = os.environ.get("EDGEFLOW_SERVER", "http://localhost:5000")
EDGEFLOW_TARGET = os.environ.get("EDGEFLOW_TARGET", "yolo-inference")

MODEL_VARIANT = os.environ.get("YOLO_VARIANT", "yolov8n")  # n / s / m / l / x
INPUT_SIZE = 640
CONF_THRESHOLD = float(os.environ.get("YOLO_CONF_THRESHOLD", "0.5"))
IOU_THRESHOLD = float(os.environ.get("YOLO_IOU_THRESHOLD", "0.7"))

# COCO 80-class labels in index order.
COCO_LABELS = [
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "backpack",
    "umbrella",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "dining table",
    "toilet",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
]

# ── export to ONNX ─────────────────────────────────────────────────────────────

print(f"loading {MODEL_VARIANT} pretrained weights (downloads on first run)...")
from ultralytics import YOLO  # noqa: E402

model = YOLO(f"{MODEL_VARIANT}.pt")

print(f"exporting to ONNX (imgsz={INPUT_SIZE}, opset=12)...")
with tempfile.TemporaryDirectory() as tmp:
    # Export writes <model>.onnx next to the weights file; capture the path.
    onnx_path = model.export(
        format="onnx",
        imgsz=INPUT_SIZE,
        opset=12,
        simplify=False,  # no onnxsim dependency
        nms=False,  # raw [1, 84, 8400] output; NMS handled in postprocess WASM
    )
    onnx_bytes = Path(onnx_path).read_bytes()

print(f"ONNX model size: {len(onnx_bytes) / 1024:.0f} KB")

# ── push to edgeflow ───────────────────────────────────────────────────────────

print(f"\npushing to edgeflow at {EDGEFLOW_SERVER}...")
mlflow.set_tracking_uri(EDGEFLOW_SERVER)
exp = mlflow.set_experiment("yolov8-object-detection")

with mlflow.start_run(
    experiment_id=exp.experiment_id, run_name=f"{MODEL_VARIANT}-coco"
) as run:
    mlflow.log_params(
        {
            "model": MODEL_VARIANT,
            "input_size": INPUT_SIZE,
            "num_classes": len(COCO_LABELS),
            "conf_threshold": CONF_THRESHOLD,
            "iou_threshold": IOU_THRESHOLD,
            "weights": "coco-pretrained",
        }
    )

    edgeflow.log_model(
        model_bytes=onnx_bytes,
        preprocess=edgeflow.Pipeline(
            [
                edgeflow.ImageToTensor(width=INPUT_SIZE, height=INPUT_SIZE),
            ]
        ),
        postprocess=edgeflow.Pipeline(
            [
                edgeflow.DetectionOutput(
                    labels=COCO_LABELS,
                    conf_threshold=CONF_THRESHOLD,
                    iou_threshold=IOU_THRESHOLD,
                    model_size=INPUT_SIZE,
                ),
            ]
        ),
    )
    run_id = run.info.run_id

print(f"run_id: {run_id}")

# ── register + deploy ──────────────────────────────────────────────────────────

mv = edgeflow.register(run_id, "yolov8n", server=EDGEFLOW_SERVER)
deployment = edgeflow.deploy(
    mv.name, mv.version, EDGEFLOW_TARGET, server=EDGEFLOW_SERVER, wait=True
)

print()
print("done. to test inference:")
print("  curl -s -X POST http://localhost:8080/infer \\")
print('    -H "content-type: image/jpeg" \\')
print("    --data-binary @photo.jpg")
print()
print("bounding boxes are normalised to [0, 1].")
print("multiply by (img_w, img_h, img_w, img_h) to get pixel coordinates.")
