Quickstart: your first deploy in 2 minutes
==========================================

This tutorial gets you from zero to a live inference endpoint. We use
the iris dataset because it trains in under a second and has no
heavyweight dependencies. Later tutorials move to real models.

You will:

1. Bring up edgeflow locally with docker compose.
2. Train a classifier and push it to edgeflow.
3. Send a request to the live inference endpoint.

Prerequisites
-------------

- Docker and docker compose
- ``uv`` (`installation guide <https://docs.astral.sh/uv/getting-started/installation/>`_)

1. Bring up edgeflow
--------------------

Pull the quickstart compose file and start the stack. No clone needed -
``quickstart.yaml`` references pre-built images on GHCR.

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/deploy/quickstart.yaml
   docker compose -f quickstart.yaml up -d

Two containers start: the control-plane ``server`` on ``:5000`` and an
``inference`` pod on ``:8080``.

2. Train and deploy
-------------------

Fetch the example training script and run it. ``train.py`` declares its
Python dependencies inline (PEP 723), so ``uv run`` resolves and caches
them in an ephemeral environment.

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/01-quickstart-iris/train.py
   uv run train.py

The script trains a ``LogisticRegression`` on iris, exports it to ONNX,
pushes it to the server via MLflow, registers a model version, and
deploys it to the ``quickstart`` target. The script blocks until the
pod confirms the model is loaded, so once it exits you can call the
endpoint immediately.

3. Send a request
-----------------

The inference endpoint accepts a JSON array of feature values
(sepal length, sepal width, petal length, petal width).

.. code-block:: bash

   curl -X POST http://localhost:8080/infer \
        -H 'Content-Type: application/json' \
        -d '[5.1, 3.5, 1.4, 0.2]'

You should get back something like::

   {"class_id":0,"label":"setosa","confidence":0.9766}

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

- :doc:`02-iris-with-preprocessing` - move feature normalisation off
  the client and into a WASM pre-transform that ships with the model.
- :doc:`03-adult-income` - JSON input with encoded categorical
  features (named-input mode).
- :doc:`05-k3d-yolo` - real CV model, image input, k3d deployment.
