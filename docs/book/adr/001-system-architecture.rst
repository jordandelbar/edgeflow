ADR-001: System Architecture
=============================

**Status:** Accepted

**Date:** 2026-03-24

--------

Context
-------

Edgeflow aims to be an MLOps platform purpose-built for edge device fleets: robots, drones, industrial hardware,
autonomous systems. The core problem it solves:

- Teams deploying ML models to physical devices have no standard tooling to know what model version is running
  on which device, detect degradation in production, and safely roll back across a fleet.

Existing tools (MLflow, Weights & Biases, BentoML) solve parts of this problem but assume a cloud or datacenter context.
None provide a coherent story for edge-constrained environments where devices may be offline,
have limited resources, and are managed as a fleet rather than individually.

The system must:

- Run the server on constrained hardware (single binary, low memory footprint)
- Communicate reliably with devices on flaky or intermittent network connections
- Be compatible with existing MLflow Python clients to lower adoption friction
- Provide a path toward a native richer API as the product matures

--------

Decision
--------

The system is composed of four distinct components with clearly separated responsibilities:

1. edgeflow-server
~~~~~~~~~~~~~~~~~~

The central control plane. Runs in the infrastructure (bare metal, VM, or k3s cluster).

**Responsibilities:**

- Experiment tracking (MLflow-compatible API surface)
- Model registry (versioning, staging, promotion)
- Deployment orchestration (targeting devices and fleets)
- Telemetry ingestion and storage
- Drift detection evaluation
- Alert rule management
- Serving the Svelte UI as static files
- MQTT broker integration (publishes deployment instructions, subscribes to telemetry)

**Stack:** Rust + Axum + SQLite (swappable via ``Store`` trait) + tower-http

2. edgeflow-inference
~~~~~~~~~~~~~~~~~~~~~

A standalone process running on each edge device. Responsible purely for model execution.

**Responsibilities:**

- Load and run ONNX models via ``ort`` (ONNX Runtime)
- Expose a local HTTP endpoint (``POST /infer``) for the application to call
- Collect input/output telemetry and latency metrics
- Detect input distribution drift against a stored baseline
- Publish telemetry to the device manager via local IPC

**Stack:** Rust + ``ort`` + minimal HTTP server (no axum, weight matters on device)

**Key design choice:** ``edgeflow-inference`` is useful standalone, without edgeflow.
A user can run it as a pure ONNX inference server with drift monitoring and never use the rest of the platform.
This lowers the barrier to adoption and makes the component independently valuable.

3. edgeflow-device
~~~~~~~~~~~~~~~~~~

A lifecycle manager running on each edge device alongside the inference service.

**Responsibilities:**

- Register the device with edgeflow-server on startup
- Send periodic heartbeats
- Receive deployment instructions from server via MQTT
- Download model artifacts from the registry
- Manage the lifecycle of ``edgeflow-inference`` (start, stop, swap model version)
- Forward telemetry from inference service to server via MQTT
- Execute rollbacks on server instruction or local degradation threshold breach

**Stack:** Rust + MQTT client (``rumqttc``) + local process management

**Key design choice:** ``edgeflow-device`` and ``edgeflow-inference`` are two separate binaries.
The device manager is purely orchestration, it does not run models.
The inference service does not know about the server — it only knows about its local model and the device manager.

4. edgeflow-ui
~~~~~~~~~~~~~~

A Svelte SPA served as static files by ``edgeflow-server``. No separate server process.

**Responsibilities:**

- Fleet overview (devices, status, what's running where)
- Model registry browser (versions, lineage, promotion workflow)
- Deployment management (create, monitor, rollback)
- Experiment and run browser
- Drift and alert visualization

**Stack:** SvelteKit + adapter-static + TypeScript

--------

Communication Architecture
---------------------------

.. image:: /_static/diagrams/001-communication.svg
   :alt: Communication architecture
   :width: 100%

--------

MQTT Topic Structure
--------------------

.. code-block:: text

    edgeflow/devices/{device_id}/register
    edgeflow/devices/{device_id}/heartbeat
    edgeflow/devices/{device_id}/deploy          ← server → device
    edgeflow/devices/{device_id}/deploy/ack      ← device → server
    edgeflow/devices/{device_id}/deploy/status   ← device → server (progress)
    edgeflow/devices/{device_id}/rollback        ← server → device
    edgeflow/devices/{device_id}/rollback/ack    ← device → server
    edgeflow/devices/{device_id}/telemetry       ← device → server

--------

API Surface
-----------

Native API (product identity)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: text

    /api/v1/devices/**
    /api/v1/fleets/**
    /api/v1/models/**
    /api/v1/deployments/**
    /api/v1/experiments/**
    /api/v1/runs/**
    /api/v1/alerts/**

MLflow Compatibility Shim (adoption layer, frozen surface)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: text

    /mlflow/api/2.0/mlflow/experiments/**
    /mlflow/api/2.0/mlflow/runs/**
    /mlflow/api/2.0/mlflow/metrics/**
    /mlflow/api/2.0/mlflow/artifacts/**

The shim is a translation layer over the native data model, not a separate implementation.
The native data model is designed first; the shim is built on top of it.

--------

Model Format
------------

ONNX is the sole supported model format for deployment.

**Rationale:**

- Universal export target from PyTorch, TensorFlow, scikit-learn, and most major frameworks
- ``ort`` (ONNX Runtime Rust bindings) is mature and actively maintained
- The ONNX graph contains full I/O schema (tensor names, shapes, dtypes) which edgeflow uses to auto-generate
  inference wrappers and telemetry configuration, no user configuration required
- Enables hardware-specific optimization (CUDA, TensorRT, CoreML)
  via ORT execution providers without changing the model artifact

--------

Drift Detection Strategy (PoC)
-------------------------------

For the initial implementation: **running mean/std shift on input tensors**.

- Baseline distribution established from training data samples pushed with the model artifact
- Agent computes running statistics on live inputs
- Drift flagged when input distribution exceeds N standard deviations from baseline
- PSI (Population Stability Index) and KL Divergence added in subsequent iterations

--------

Key Lineage Chain
-----------------

The central value proposition of edgeflow is full lineage from training to device:

.. image:: /_static/diagrams/001-lineage.svg
   :alt: Key lineage chain
   :width: 100%

This chain must be queryable end to end. A user must be able to ask:

- *"What training run produced the model on robot-07?"*
- *"Which devices are running a model trained on dataset version X?"*
- *"What changed between the model on robot-07 and robot-12?"*

--------

Consequences
------------

**Positive:**

- Two-binary device design means ``edgeflow-inference`` can be adopted standalone, lowering the barrier to entry
- MQTT handles flaky edge networks gracefully (QoS, persistent sessions, offline buffering)
- MLflow shim provides immediate compatibility with existing Python training code
- Single Rust binary for the server is trivially deployable on constrained hardware
- ``Store`` trait abstraction allows SQLite → Postgres migration without changing business logic

**Negative / risks:**

- MQTT adds an operational dependency (broker must be running), mitigated by bundling a lightweight broker (rumqttd)
  in the server binary for single-node deployments
- ONNX-only model format excludes teams using TensorFlow SavedModel or TorchScript directly, acceptable tradeoff for
  PoC, revisit at v1
- Maintaining MLflow API compatibility is ongoing work as MLflow evolves treat the shim as frozen at the current
  API surface, do not track MLflow HEAD

--------

Alternatives Considered
------------------------

**gRPC instead of MQTT for device communication**

Rejected. gRPC assumes stable connections. MQTT's QoS levels and persistent sessions are designed for exactly the intermittent connectivity pattern of edge devices.

**Single binary on device (device manager + inference in one process)**

Rejected. ``edgeflow-inference`` has standalone value. Merging the two would force users to adopt the full edgeflow ecosystem to get a managed ONNX inference server, which raises the adoption barrier unnecessarily.

**REST polling instead of MQTT**

Rejected for device communication. Polling introduces latency in deployment instructions and wastes bandwidth on heartbeats. Acceptable for the UI/API layer where HTTP semantics are natural.
