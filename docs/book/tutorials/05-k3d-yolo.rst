YOLOv8 on edgeflow: image input, WASM pre/post
==============================================

So far the tutorials have been tabular. Real edge ML is mostly
computer vision, and CV models share a few annoying properties:

- The input is opaque bytes (JPEG, PNG) that the model can't ingest
  directly; you need to decode, resize, and re-layout the channels.
- The output is dense (YOLOv8n returns ``[1, 84, 8400]``) and needs
  non-max suppression before it's useful.
- The artifact is bigger - around 12 MB of ONNX weights plus the COCO
  label list.

This is where edgeflow's WASM pre/post transforms earn their keep.
The model and its image plumbing ship as one artifact; the client
sends raw JPEG bytes and gets back a JSON list of detected objects.

You will:

1. Pull a pretrained YOLOv8n from ultralytics, export it to ONNX.
2. Attach an ``ImageToTensor`` pre-transform (decode, resize, NHWC->NCHW)
   and a ``DetectionOutput`` post-transform (NMS, normalised bboxes).
3. Send a JPEG and get back labelled bounding boxes.

Prerequisites
-------------

- Edgeflow running via docker compose (or k3d - see "Going to k3d"
  below).
- Python 3.12+ and ``uv``.
- ~200 MB of disk for ``ultralytics`` and its torch dependency.

1. Deploy YOLOv8
----------------

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/05-k3d-yolo/deploy.py
   uv run deploy.py

The script downloads ``yolov8n.pt`` (~6 MB) on first run, exports it
to ONNX with ``imgsz=640, opset=12, nms=False`` (NMS happens in the
postprocess WASM), and registers it under the model name ``yolov8n``.
It also attaches the COCO 80-class label list to ``DetectionOutput``
so responses come back with human-readable labels, not class indices.

The pre/post pipeline composition is the heart of the tutorial:

.. literalinclude:: ../../../examples/05-k3d-yolo/deploy.py
   :language: python
   :start-after: # [docs:start:log-model]
   :end-before: # [docs:end:log-model]
   :dedent:

``ImageToTensor`` decodes JPEG/PNG, resizes to 640x640, and switches
to NCHW layout. ``DetectionOutput`` runs NMS, maps class indices to
COCO labels, and emits the JSON response. Both run as WASM components
inside the inference pod; no Python on the request path.

Expected output:

.. code-block:: text

   loading yolov8n pretrained weights (downloads on first run)...
   exporting to ONNX (imgsz=640, opset=12)...
   ONNX model size: 12000 KB
   pushing to edgeflow at http://localhost:5000...

2. Send a JPEG
--------------

Grab any photo. The pipeline accepts both JPEG and PNG; bytes go
straight to the inference endpoint with no client-side preprocessing.

.. code-block:: bash

   curl -s -X POST http://localhost:8080/infer \
     -H "content-type: image/jpeg" \
     --data-binary @photo.jpg

Response:

.. code-block:: json

   [
     {"class_id": 0, "label": "person",  "confidence": 0.9134, "bbox": [0.23, 0.18, 0.67, 0.92]},
     {"class_id": 2, "label": "car",     "confidence": 0.8701, "bbox": [0.01, 0.42, 0.38, 0.81]}
   ]

Bounding boxes are normalised to ``[0, 1]``. Multiply by your display
dimensions (``w, h, w, h``) to get pixel coordinates.

What just happened?
-------------------

- ``ImageToTensor(width=640, height=640)`` is a standard WASM
  component shipped with edgeflow. It decodes JPEG/PNG, resizes with
  bilinear interpolation, and flips channel order to NCHW. The full
  pipeline runs in WASM; no Python on the inference path.
- ``DetectionOutput`` runs NMS (configurable IoU and confidence
  thresholds), maps class indices to COCO labels, and emits the JSON
  response.
- End-to-end latency on CPU is around 60 ms per image (median).

Performance numbers
-------------------

For reference, end-to-end on a single ``inference`` pod, no
batching, raw 1080p JPEG input:

- Median latency: ~58 ms
- WASM boundary cost: ~2x ``memcpy``
- Memory at rest: under 100 MB
- Memory under load: depends on concurrency, typically 150-250 MB

Going to k3d
------------

The same artifact runs unchanged on a k3d cluster. The control plane
moves from docker compose to a k3d-managed deployment, and edgeflow
creates ``Deployment`` + ``Service`` objects for the inference pod
automatically when you call ``edgeflow.deploy``. See the
``deploy/k3d-cluster.yaml`` config and the ``just up`` recipe.
A dedicated k3d tutorial is planned.

Next steps
----------

- Add custom standard layers (``TopKOutput``, ``EmbeddingNormalize``)
  for your own model output shape.
- Write a Rust WASM transform from scratch when the standard layers
  do not fit (planned: dedicated how-to guide).
