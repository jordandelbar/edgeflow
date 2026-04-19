Quickstart: your first deploy in 2 minutes
==========================================

This tutorial gets you from zero to a live inference endpoint. We use
the iris dataset because it trains in under a second and has no
heavyweight dependencies. Later tutorials move to real models.

You will:

1. Bring up edgeflow locally with docker compose.
2. Train a classifier and push it to edgeflow.
3. Send a request to the live inference endpoint.

No git clone required.

Prerequisites
-------------

- Docker and docker compose
- Python 3.12+
- ``uv`` (or ``pip``) to install the ``edgeflow`` client

1. Bring up edgeflow
--------------------

Pull the compose file straight from the repository and start the stack:

.. code-block:: bash

   curl -fsSL https://raw.githubusercontent.com/jordandelbar/edgeflow/main/deploy/docker-compose.yaml \
     -o edgeflow-compose.yaml
   docker compose -f edgeflow-compose.yaml up -d

Two containers start: the control-plane ``server`` on ``:5000`` and an
``inference`` pod on ``:8080``.

2. Train and deploy
-------------------

Fetch the training script and run it:

.. code-block:: bash

   curl -fsSL https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/01-quickstart-iris/train.py \
     -o train.py
   uv run --with edgeflow --with scikit-learn --with mlflow train.py

The script trains a ``LogisticRegression`` on iris, exports it to ONNX,
pushes it to the server via MLflow, registers a model version, and
deploys it to the ``quickstart`` target. Expect output like::

   accuracy: 0.9667
   pushing to edgeflow at http://localhost:5000...
   run_id: a1b2c3...

3. Send a request
-----------------

The inference endpoint takes raw bytes: ``4 x f32`` little-endian
(sepal length, sepal width, petal length, petal width).

.. code-block:: bash

   python3 -c "import struct, sys; sys.stdout.buffer.write(struct.pack('<4f', 5.1, 3.5, 1.4, 0.2))" \
     | curl -s -X POST http://localhost:8080/infer --data-binary @-

You should get back a JSON response with a predicted class label.

What just happened?
-------------------

- ``edgeflow.log_model`` shipped the ONNX bytes plus a postprocessing
  ``Pipeline`` (here, ``ClassifierOutput`` that maps logits to labels).
- ``edgeflow.register`` created a versioned artifact in the model
  registry.
- ``edgeflow.deploy`` told the ``quickstart`` target to load that
  version. MQTT carried the command to the inference pod.

Next steps
----------

- Tutorial 02: JSON input instead of raw bytes, using named-input mode.
- How-to: hot-swap a model without downtime.
