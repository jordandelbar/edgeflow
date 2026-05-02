Edgeflow
========

Train in Python. Serve in Rust.

Edgeflow is an MLflow-compatible experiment tracker, model registry,
and inference server, built for nodes where a Python serving stack is
too heavy to fit. Models log through the MLflow client you already
use; the runtime is a single Rust binary that loads ONNX, runs WASM
pre/post processing, and hot-swaps deployments without dropping
traffic.

Why it exists
-------------

A Python serving container easily passes several hundred MB resident
before it answers a single request. On a constrained node - an edge
box, a small VPS, a free tier - that is the difference between fitting
your model and not. Edgeflow keeps the authoring ergonomics teams
already have (MLflow tracking, a Python SDK, ONNX export) but moves
inference into a Rust + WASM runtime measured in tens of MB.

Hot-swap, per-target observability, and multi-target deployments are
first-class because rebuilding and pushing a container image is not a
realistic update mechanism for a node living behind a flaky link in a
warehouse.

Who it's for
------------

- ML engineers shipping models to constrained nodes who want to keep
  their MLflow workflow.
- Teams running fleets of edge devices who do not want to pay the
  per-node weight of a full Python serving stack.
- Anyone building a side project or proof of concept that needs a
  serving story lighter than a full container orchestration deploy.

Where to start
--------------

**New here.** Walk through the :doc:`Quickstart tutorial
<tutorials/01-quickstart-iris>`. About two minutes from zero to a
live ``/infer`` endpoint.

**Evaluating edgeflow.** Skim the :doc:`system architecture
<adr/001-system-architecture>` to see how the control plane, the
inference runtime, and the artifact store fit together.

**Building on the API.** A dedicated reference section is in flight.
For now, the tutorials cover the SDK and HTTP surface end-to-end on
real models.

.. toctree::
   :hidden:
   :caption: Tutorials

   tutorials/01-quickstart-iris
   tutorials/02-iris-with-preprocessing
   tutorials/03-adult-income
   tutorials/05-k3d-yolo

.. toctree::
   :hidden:
   :caption: Architecture

   adr/001-system-architecture
   adr/002-wasm-inference-transforms
