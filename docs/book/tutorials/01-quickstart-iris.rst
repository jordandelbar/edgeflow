Quickstart: your first deploy in 2 minutes
==========================================

This tutorial gets you from zero to a live inference endpoint. We use
the iris dataset because it trains in under a second and has no
heavyweight dependencies. Later tutorials move to real models.

You will:

1. Bring up edgeflow locally with docker compose.
2. Walk through a training script that pushes the model to edgeflow.
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

2. Build the training script
----------------------------

If you just want to see it work, the finished script is on GitHub:

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/01-quickstart-iris/train.py
   uv run train.py

The rest of this section walks through ``train.py`` piece by piece.

Dependencies
~~~~~~~~~~~~

Create a ``pyproject.toml`` in your project directory with these
dependencies:

.. literalinclude:: ../../../examples/01-quickstart-iris/pyproject.toml
   :language: toml
   :start-after: # [docs:start:deps]
   :end-before: # [docs:end:deps]

Imports
~~~~~~~

Three groups: edgeflow's SDK, MLflow for experiment tracking, and
scikit-learn for the model itself.

.. literalinclude:: ../../../examples/01-quickstart-iris/train.py
   :language: python
   :start-after: # [docs:start:imports]
   :end-before: # [docs:end:imports]

``edgeflow.models.sklearn_to_onnx`` is a small helper that wraps
``skl2onnx`` with sensible defaults. You can call ``skl2onnx``
directly if you need finer control.

Configuration
~~~~~~~~~~~~~

Two values point the script at the edgeflow server and pick a
deployment target. The defaults match what ``docker compose up``
exposes, so you only need to override them when running against a
different setup.

.. literalinclude:: ../../../examples/01-quickstart-iris/train.py
   :language: python
   :start-after: # [docs:start:config]
   :end-before: # [docs:end:config]

Train the classifier
~~~~~~~~~~~~~~~~~~~~

Standard scikit-learn flow. Iris loads from a built-in dataset; cast
features to ``float32`` because that's what the ONNX exporter expects.

.. literalinclude:: ../../../examples/01-quickstart-iris/train.py
   :language: python
   :start-after: # [docs:start:train]
   :end-before: # [docs:end:train]

Nothing edgeflow-specific yet - this is the same code you would write
to fit and evaluate any sklearn classifier.

Log to MLflow and bundle the model
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Edgeflow speaks the MLflow tracking protocol. Point ``mlflow`` at the
edgeflow server and start a run as you normally would; ``log_params``
and ``log_metric`` behave exactly as they would against a vanilla
MLflow server.

The edgeflow-specific call is ``edgeflow.log_model``. It serialises
the trained classifier to ONNX and bundles it with a postprocess
pipeline into a single artifact. Here the pipeline is just
``ClassifierOutput``, which maps the model's raw probability vector to
``{class_id, label, confidence}`` so the inference endpoint returns
something humans can read.

.. literalinclude:: ../../../examples/01-quickstart-iris/train.py
   :language: python
   :start-after: # [docs:start:mlflow-run]
   :end-before: # [docs:end:mlflow-run]

The bundled artifact is the unit edgeflow loads into an inference pod
later. Both the ONNX bytes and the postprocess pipeline travel
together; the pod has everything it needs from a single download.

Register and deploy
~~~~~~~~~~~~~~~~~~~

The MLflow run is a record of the experiment. To make the model
addressable, promote it into the registry, then point a target at it.

.. literalinclude:: ../../../examples/01-quickstart-iris/train.py
   :language: python
   :start-after: # [docs:start:register-deploy]
   :end-before: # [docs:end:register-deploy]

``register`` creates a versioned ``ModelVersion`` from the run.
``deploy`` tells the ``quickstart`` target to load that version, and
``wait=True`` blocks until the inference pod confirms the new pipeline
is live - so by the time the script exits, you can hit the endpoint.

Expected output:

.. code-block:: text

   training iris classifier...
   accuracy: 0.9667
   pushing to edgeflow at http://localhost:5000...
   run_id: 1f2a...

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

Next steps
----------

- :doc:`02-iris-with-preprocessing` - move feature normalisation off
  the client and into a WASM pre-transform that ships with the model.
- :doc:`03-adult-income` - JSON input with encoded categorical
  features (named-input mode).
- :doc:`05-k3d-yolo` - real CV model, image input, k3d deployment.
