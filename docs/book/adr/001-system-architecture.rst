ADR-001: System Architecture
=============================

**Status:** Accepted

**Date:** 2026-03-28 (revised from 2026-03-24)

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
- Not require additional infrastructure to get started (broker, message queue, sidecar)
- Provide a path toward a native richer API as the product matures

--------

Decision
--------

The system is composed of **two components** with clearly separated responsibilities.
A third component (device manager / lifecycle supervisor) was considered and rejected — see Alternatives Considered.

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
- Embedded MQTT broker (``rumqttd``) for device communication

**Stack:** Rust + Axum + SQLite (swappable via ``Store`` trait) + tower-http + rumqttd

**Key design choice:** the MQTT broker is embedded in the server binary via ``rumqttd``.
No separate broker process is required. For deployments that already operate a managed broker
(AWS IoT, HiveMQ, Mosquitto), the embedded broker can be disabled and an external URL configured instead:

.. code-block:: toml

    [mqtt]
    broker = "embedded"          # default — runs rumqttd inside the server process
    # broker = "mqtt://your-broker.example.com:1883"   # external alternative

2. edgeflow-inference
~~~~~~~~~~~~~~~~~~~~~

A standalone process that runs on each target (edge device, VM, k8s pod, or bare metal server).
Responsible for model execution and, in managed mode, for its own lifecycle coordination with the server.

**Responsibilities:**

- Load and run ONNX models via ``ort`` (ONNX Runtime)
- Expose a local HTTP endpoint (``POST /infer``) for the application to call
- Collect input/output telemetry and latency metrics
- Detect input distribution drift against a stored baseline

In **managed mode** (when ``[server]`` is configured):

- Register with ``edgeflow-server`` on startup
- Send periodic heartbeats
- Listen for deployment instructions (MQTT subscription or HTTP poll fallback)
- Download model artifacts from the registry
- Hot-swap the loaded model without downtime
- Confirm deployment status back to the server

**Stack:** Rust + ``ort`` + minimal HTTP server (no axum, weight matters on device) + optional MQTT client

**Two modes, one binary:**

.. code-block:: toml

    # Standalone — pure inference server, no server communication
    # (omit [server] block entirely)

    # Managed — full lifecycle coordination
    [server]
    url  = "http://edgeflow-server:5000"
    mqtt = "mqtt://edgeflow-server:1883"   # optional; falls back to HTTP poll if omitted

**Key design choice:** ``edgeflow-inference`` is useful standalone, without edgeflow-server.
A user can run it as a pure ONNX inference server and never use the rest of the platform.
This lowers the barrier to adoption and makes the component independently valuable.
Process supervision (restart on crash) is delegated to the OS layer (systemd, Docker restart policy)
rather than implemented as a third binary.

--------

Communication Architecture
---------------------------

.. image:: /_static/diagrams/001-communication.svg
   :alt: Communication architecture
   :width: 100%

**Transport evolution:**

The managed communication path is designed to evolve without breaking changes:

1. **HTTP pull (v1, no extra infrastructure)** — inference polls server for pending deployments.
   Works through NAT and firewalls. Acceptable latency for model deployment (not control-plane timing-critical).

2. **MQTT (v2, embedded broker)** — inference subscribes to its deployment topic.
   Server publishes instructions immediately on deployment creation.
   Better for large fleets and real-time rollback instructions.

Both transports can coexist. Devices without MQTT connectivity fall back to HTTP poll automatically.

--------

MQTT Topic Structure
--------------------

.. code-block:: text

    edgeflow/targets/{target_id}/deploy          ← server → inference (deployment instruction)
    edgeflow/targets/{target_id}/deploy/ack      ← inference → server (accepted, loading)
    edgeflow/targets/{target_id}/deploy/status   ← inference → server (deployed / failed)
    edgeflow/targets/{target_id}/heartbeat       ← inference → server
    edgeflow/targets/{target_id}/telemetry       ← inference → server

--------

API Surface
-----------

Native API (product identity)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: text

    /api/v1/targets/**       ← target registration, heartbeat, pending deployments
    /api/v1/deployments/**
    /api/v1/models/**
    /api/v1/experiments/**
    /api/v1/runs/**
    /api/v1/alerts/**

MLflow Compatibility Shim (adoption layer, frozen surface)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: text

    /api/2.0/mlflow/experiments/**
    /api/2.0/mlflow/runs/**
    /api/2.0/mlflow/metrics/**
    /api/2.0/mlflow/artifacts/**

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

- Two-component design keeps the operational surface minimal — one server binary, one inference binary per device
- ``edgeflow-inference`` can be adopted standalone, lowering the barrier to entry
- Embedded MQTT broker means zero extra infrastructure for the common case
- HTTP poll fallback means the system works through NAT and firewalls out of the box
- MQTT handles flaky edge networks gracefully (QoS, persistent sessions, offline buffering) when available
- MLflow shim provides immediate compatibility with existing Python training code
- Single Rust binary for the server is trivially deployable on constrained hardware
- ``Store`` trait abstraction allows SQLite → Postgres migration without changing business logic

**Negative / risks:**

- ``edgeflow-inference`` now carries both inference logic and lifecycle coordination — these must be kept cleanly separated internally so standalone mode remains truly standalone
- ONNX-only model format excludes teams using TensorFlow SavedModel or TorchScript directly; acceptable tradeoff for PoC, revisit at v1
- Maintaining MLflow API compatibility is ongoing work as MLflow evolves; treat the shim as frozen at the current API surface, do not track MLflow HEAD

--------

Alternatives Considered
------------------------

**Separate ``edgeflow-device`` binary (device manager)**

Rejected. The responsibilities originally assigned to a separate device manager —
registration, heartbeats, artifact download, hot-swap, deployment confirmation —
are already implemented in ``edgeflow-inference``.
The only remaining concern (process restart on crash) is better handled by systemd or Docker restart policies
than by a custom-written process supervisor.
A third binary adds operational surface (a third systemd unit, a third thing to monitor and update)
with no architectural benefit.

**gRPC instead of MQTT for device communication**

Rejected. gRPC assumes stable connections. MQTT's QoS levels and persistent sessions are designed for exactly the intermittent connectivity pattern of edge devices.

**REST polling instead of MQTT**

Not fully rejected — HTTP poll is the v1 transport precisely because it requires no extra infrastructure
and works through NAT. MQTT is the v2 transport that replaces polling once the embedded broker is in place.
The two coexist: devices without MQTT connectivity continue to poll.

**External MQTT broker (Mosquitto, HiveMQ) as a required dependency**

Rejected for the default case. Early adopters should not have to operate a broker to get started.
``rumqttd`` embedded in the server binary provides a real MQTT broker with zero operational overhead.
External brokers remain supported for production deployments that already have one.
