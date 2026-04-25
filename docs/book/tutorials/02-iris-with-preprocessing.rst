Iris with preprocessing: ship transforms with the model
=======================================================

In tutorial 01 the client sent a JSON array of features. In production
those features usually need normalisation, scaling, or encoding before
they hit the model. The naive approach is to do that work in the client,
but then every consumer has to ship the same preprocessing logic and
stay in sync with the model.

Edgeflow's answer: bake the preprocessing into the deployment artifact
itself, as a WASM pre-transform. The client keeps sending the same
request; the inference server runs the transform inside the pipeline,
just before the model. Hot-swap a new model with new normalisation
parameters and the client never knows.

You will:

1. Train a LogisticRegression on z-scored iris features.
2. Attach a ``Normalize`` WASM pre-transform with the per-feature mean
   and std baked in.
3. Send the same JSON array as tutorial 01 - and get the right answer.

Prerequisites
-------------

- Tutorial 01 working, or at least edgeflow up via docker compose.
- Python 3.12+ and ``uv``.

1. Bring up edgeflow
--------------------

Same as tutorial 01. If the stack is already running, skip ahead.

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/deploy/quickstart.yaml
   docker compose -f quickstart.yaml up -d

2. Train with preprocessing baked in
------------------------------------

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/02-iris-with-preprocessing/train.py
   uv run train.py

The script computes per-feature mean and std on the training set,
trains a ``LogisticRegression`` on z-scored features, and pushes the
model along with an ``edgeflow.Normalize(mean=..., std=...)``
pre-transform. Output:

.. code-block:: text

   feature mean: [5.84, 3.05, 3.74, 1.20]
   feature std:  [0.83, 0.43, 1.77, 0.76]
   training on z-scored features...
   accuracy: 0.9667
   pushing to edgeflow at http://localhost:5000...

3. Send un-normalised features
------------------------------

Send the same JSON array as tutorial 01, with the raw (un-normalised)
feature values:

.. code-block:: bash

   curl -X POST http://localhost:8080/infer \
        -H 'Content-Type: application/json' \
        -d '[5.1, 3.5, 1.4, 0.2]'

The server runs your input through the WASM ``Normalize`` transform
first, then through the ONNX model. You get back the same labelled
prediction format as tutorial 01.

What just happened?
-------------------

- The pre-transform was compiled to WASM at ``log_model`` time and
  bundled with the ONNX bytes into a single deployment artifact.
- The inference pod loaded the artifact, instantiated a
  ``wasmtime`` component for the pre-transform, and now runs it on
  every request before reaching the backend.
- Cost: ~2x ``memcpy`` at the WASM boundary thanks to the bindgen'd
  component model. For a 4-feature tensor this is essentially free.

Try this
--------

Train a second version with a deliberately wrong mean (say, all zeros)
and run ``train.py`` again. The new version is registered as ``v2``
and the ``iris-inference`` target hot-swaps to it. The client keeps
sending the same JSON; the predictions go bad. Roll back by deploying
``v1`` again. No client change, no downtime.

Next steps
----------

- :doc:`03-adult-income` - swap the positional array for a JSON object
  with named fields plus categorical encodings.
- :doc:`05-k3d-yolo` - same pre/post-transform pattern, but with real
  image data and a 6 MB model.
