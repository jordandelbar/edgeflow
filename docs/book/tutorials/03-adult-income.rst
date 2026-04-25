Adult income: JSON input with mixed feature types
=================================================

Most real tabular models have categorical features. A positional JSON
array of floats falls apart the moment one of those fields is
``"Bachelors"`` instead of a number; you have to map strings to integers
first. Tutorial 02 handled numeric normalisation; this tutorial handles
the harder case: a model that mixes string categoricals with numerics,
and a client that wants to send a plain JSON object without knowing the
encoding tables.

Edgeflow handles this with **named-input mode**: the client sends JSON
with named fields, the server applies the encoding tables stored in
``schema.json`` to produce a flat float tensor, and the model sees the
same shape it saw during training.

You will:

1. Train an XGBoost classifier on the UCI Adult Income dataset, with
   ``OrdinalEncoder`` for categoricals.
2. Push the column transformer to edgeflow so its encoding tables
   become part of the deployment.
3. Hit the model with a JSON request like a real API client would.

Prerequisites
-------------

- Edgeflow running via docker compose (see tutorial 01).
- Python 3.12+ and ``uv``.

1. Train and deploy
-------------------

.. code-block:: bash

   curl -O https://raw.githubusercontent.com/jordandelbar/edgeflow/main/examples/03-adult-income/train.py
   uv run train.py

The script:

- Pulls the UCI Adult Income CSV directly from
  ``archive.ics.uci.edu``.
- Splits train/test, builds a ``ColumnTransformer`` with
  ``OrdinalEncoder`` for the 8 categorical columns and passthrough for
  the 6 numerical columns.
- Trains an ``XGBClassifier``.
- Calls ``edgeflow.log_model`` with both the ONNX model **and** the
  column transformer, so its encoding tables are written into
  ``schema.json``.

The column transformer is the hinge of named-input mode:

.. literalinclude:: ../../../examples/03-adult-income/train.py
   :language: python
   :start-after: # [docs:start:column-transformer]
   :end-before: # [docs:end:column-transformer]
   :dedent:

Edgeflow introspects this object to derive the per-field encoding
tables. The ``log_model`` call passes it alongside the ONNX bytes:

.. literalinclude:: ../../../examples/03-adult-income/train.py
   :language: python
   :start-after: # [docs:start:log-model]
   :end-before: # [docs:end:log-model]
   :dedent:

Expected output:

.. code-block:: text

   model type: xgboost
   fetching adult income dataset from https://archive.ics.uci.edu/...
   dataset: 32,561 rows, 14 features
   class balance: 24.1% >50K
   training xgboost...
   F1: 0.7095  AUC-ROC: 0.9285
   pushing to edgeflow at http://localhost:5000...

2. Send a JSON request
----------------------

.. code-block:: bash

   curl -s -X POST http://localhost:8080/infer \
     -H "content-type: application/json" \
     -d '{
       "workclass": "Private",
       "education": "Bachelors",
       "marital-status": "Married-civ-spouse",
       "occupation": "Exec-managerial",
       "relationship": "Husband",
       "race": "White",
       "sex": "Male",
       "native-country": "United-States",
       "age": 45,
       "fnlwgt": 200000,
       "education-num": 13,
       "capital-gain": 0,
       "capital-loss": 0,
       "hours-per-week": 40
     }'

You get back the predicted label (``>50K`` or ``<=50K``) along with the
class probabilities.

What just happened?
-------------------

When you called ``log_model``, edgeflow introspected the
``ColumnTransformer`` and wrote each field's dtype and encoding into
a ``schema.json`` artifact bundled with the ONNX bytes: an ordinal
map for categoricals, passthrough for numerics.

When the inference pod loaded that artifact, the schema told it to
expect JSON objects keyed by field name (named-input mode) rather
than the positional float array tutorials 01 and 02 used. On each
request the server parses the JSON, looks up each categorical value
in its encoding table, and assembles a flat ``f32`` tensor in the
order the model expects - all before the ONNX session sees a single
byte.

Unknown categories
------------------

The encoder is configured with ``unknown_value=-1``. Send
``"workclass": "ImaginaryJob"`` and the request still succeeds; the
model just sees ``-1`` for that feature. This matters in production:
real client data has values you have never seen during training, and
silently failing closed beats a 500 error.

Next steps
----------

- :doc:`05-k3d-yolo` - image inputs (raw JPEG/PNG bytes), WASM
  pre-transform that decodes and resizes, postprocess that runs NMS.
