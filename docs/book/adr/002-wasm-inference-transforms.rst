ADR-002: WASM Modules for Inference Pre/Post Processing
========================================================

**Status:** Proposed

**Date:** 2026-03-25

--------

Context
-------

``edgeflow-inference`` currently loads an ONNX model and exposes a ``POST /infer`` endpoint. The caller is expected to send tensors in the exact shape and dtype the model expects, and to interpret raw output tensors directly.

In practice, this is a significant friction point. Real edge applications sit between messy sensor data and business logic:

- Input data arrives in sensor-native formats (raw image buffers, LiDAR point clouds, IMU readings) and must be normalized, resized, or windowed before the model can consume it
- Model outputs are raw tensors (logit vectors, bounding box coordinates, regression values) that must be thresholded, decoded, or post-filtered before the application can act on them
- These transforms are model-specific and tightly coupled to the training pipeline - they belong with the model artifact, not embedded in the application

Today, users are forced to handle this in their application code (ROS nodes, robot software). The same logic ends up duplicated across devices, and model updates become fragile: changing the model usually means updating application code on every device at the same time.

--------

Decision
--------

Attach WASM modules as versioned artifacts to model versions in the registry. ``edgeflow-inference`` loads and executes these modules around the ONNX inference call using a sandboxed WASM runtime (Wasmtime).

The execution model
~~~~~~~~~~~~~~~~~~~

Each model version in the registry may optionally carry two WASM artifacts:

- ``preprocess.wasm`` - transforms raw application input into tensors ready for the ONNX model
- ``postprocess.wasm`` - transforms raw ONNX output tensors into structured application-level results

The inference call flow becomes:

.. image:: /_static/diagrams/002-inference-flow.svg
   :alt: Inference call flow
   :width: 100%

Both modules are optional and independently versioned. A model version can have a preprocessor, a postprocessor, both, or neither.

Developer experience
~~~~~~~~~~~~~~~~~~~~

The primary authoring interface is Python. Users write their transforms as decorated functions alongside their training script - the same functions they already use during training and evaluation:

.. code-block:: python

    # transforms.py - lives next to the training script

    from edgeflow.transforms import preprocess, postprocess
    import numpy as np

    @preprocess
    def prepare(raw: bytes) -> np.ndarray:
        img = decode_jpeg(raw)
        return normalize(img)

    @postprocess
    def interpret(output: np.ndarray) -> bytes:
        label = LABELS[output.argmax()]
        confidence = float(output.max())
        return json.dumps({"label": label, "confidence": confidence}).encode()

The CLI compiles these to WASM and uploads the full bundle atomically:

.. code-block:: bash

    edgeflow model push \
      --name defect-classifier \
      --version 1.2.0 \
      --model model.onnx \
      --transforms transforms.py

The user never interacts with ``.wasm`` files directly. For advanced users (Rust, Go, C), pre-compiled ``.wasm`` files can be passed instead of a Python source file - same CLI flag, the CLI detects the artifact type.

WASM module interface
~~~~~~~~~~~~~~~~~~~~~

Modules expose a single exported function:

.. code-block:: text

    transform(input_ptr: i32, input_len: i32) -> i32   // returns pointer to output in linear memory

Input and output are passed as byte slices through WASM linear memory. The host (``edgeflow-inference``) owns allocation. The ABI is defined in a shared ``edgeflow-transform-sdk`` crate; the Python SDK decorators and Rust SDK macros generate the ABI boilerplate so authors work purely with typed function signatures.

Tensor serialization format
~~~~~~~~~~~~~~~~~~~~~~~~~~~

Between the preprocess module and ``ort``, and between ``ort`` and the postprocess module, tensors are serialized as a flat binary buffer with a lightweight header:

.. code-block:: text

    [ ndim: u8 | shape: [u32; ndim] | dtype: u8 | data: [u8] ]

This format is handled entirely by the SDK - transform authors work with numpy arrays on the Python side and typed slices on the Rust side. The format is intentionally minimal to avoid overhead on constrained hardware. Arrow or ONNX tensor proto may be evaluated in later iterations if batch inference becomes a requirement.

Artifact delivery and hot-swap
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

WASM modules are versioned artifacts stored in the registry alongside the ``.onnx`` file. When a deployment is triggered:

.. image:: /_static/diagrams/002-hotswap.svg
   :alt: Artifact delivery and hot-swap
   :width: 100%

The inference binary itself never changes during a model update. Only the artifact bundle is replaced. This is a much smaller operation than a binary redeploy across a fleet on a constrained network.

Sandboxing
~~~~~~~~~~

Modules run inside Wasmtime with a restricted capability set:

- No filesystem access
- No network access
- No host function imports beyond the ABI surface (memory allocation helpers, logging)
- Execution time limit enforced per call

A buggy or malicious module cannot crash the inference process or access device resources outside the transform call.

--------

Python compilation backend
--------------------------

The Python decorator interface is the stable user-facing contract. The WASM compilation backend is treated as an implementation detail and may vary:

**Primary: componentize-py**

Compiles Python source to a WASM component. Requires no Python runtime on the device. Produces self-contained ``.wasm`` artifacts. The main risk is maturity - componentize-py is actively developed but not yet proven at production scale. Must be validated early with realistic transforms (JPEG decode, numpy normalization) before committing to this path.

**Fallback A: RustPython in WASM**

RustPython is a Python interpreter written in pure Rust that compiles to WASM32. The CLI wraps the user's Python source in a RustPython-based WASM module. Same DevEx, different runtime. The constraint: RustPython does not support C extensions, so numpy and opencv are unavailable inside the transform. Applicable when transforms are pure Python (reshaping, thresholding, JSON encoding) - which covers most classical ML use cases.

**Fallback B: Native Python subprocess**

If neither WASM compilation path is viable, the CLI serializes the Python function via cloudpickle and ships it as a native artifact. ``edgeflow-inference`` spawns a Python subprocess for transform calls. Loses sandboxing and requires Python on the device - unacceptable on the most constrained hardware but viable as a temporary path while WASM tooling matures. The DevEx (decorator interface, CLI push command) is identical.

The choice of backend is transparent to the user. The CLI selects the best available backend and records which backend was used in the artifact metadata stored in the registry.

--------

Consequences
------------

**Positive:**

- Pre/post processing logic ships with the model artifact - application code no longer needs to change when the model changes
- The same Python functions used at training time run on the device, so transforms cannot drift between training and inference
- Modules are sandboxed: a broken transform returns an error, it does not crash ``edgeflow-inference``
- Model updates are artifact swaps, not binary redeploys - smaller, faster, safer over constrained fleet networks
- Extends the lineage chain: the transform module version is part of the model version record
- ``edgeflow-inference`` remains useful standalone - modules are optional, the ONNX-only path still works

**Negative / risks:**

- Wasmtime adds binary weight to ``edgeflow-inference`` - must be validated on the most constrained target hardware before committing
- The transform ABI is a new stable API surface - breaking changes force users to recompile all modules
- Execution time limits require tuning: too tight breaks legitimate heavy transforms (image decoding), too loose opens resource exhaustion risk on the device
- componentize-py maturity is an open risk - early prototype required to validate before the Python path is advertised as supported
- RustPython's lack of C extension support means numpy-heavy transforms fall back to the subprocess path on constrained devices

--------

Alternatives Considered
------------------------

**Shared library plugins (.so / .dylib)**

Rejected. Native plugins are not sandboxed - a crash or segfault takes down the inference process. Requires per-architecture compilation. WASM provides equivalent extensibility with isolation and a single portable artifact.

**PyO3 as compilation intermediary**

Considered as a fallback if componentize-py is not viable. Rejected: PyO3 embeds CPython, which cannot compile to WASM32 due to native C dependencies. PyO3 is the right tool for Rust↔Python interop on native targets but cannot produce self-contained WASM modules. RustPython is the correct Rust-based fallback for pure Python transforms.

**Transform logic embedded in the application (status quo)**

Rejected as a long-term solution. Forces application code changes on model updates, duplicates logic across devices, and breaks the clean separation between model artifacts and application software.

**gRPC sidecar for transforms**

Rejected. Introduces a network hop on the critical inference path and significant operational overhead for what is a pure data transformation.

**Running transforms inside the ONNX graph**

Partially valid - ONNX supports preprocessing ops and some postprocessing. However, ONNX graph manipulation requires Python tooling, is opaque to non-ML engineers, and cannot express arbitrary logic (protocol buffer decoding, sensor-specific calibration). WASM complements ONNX rather than replacing it.
