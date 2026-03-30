"""
Generate a minimal ONNX fixture for inference benchmarks and tests.

The model is a tiny linear classifier (Gemm + Softmax) with:
  input  : float32 [N, 4]   — iris-like features
  output : float32 [N, 3]   — softmax class probabilities

Because the output is already float32 at index 0, it works directly with the
ORT backend without any postprocess WASM.

Dependencies (install once):
    pip install onnx numpy

Usage (from repo root):
    python scripts/gen_bench_model.py
"""

import os

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

# ── reproducible weights ─────────────────────────────────────────────────────
rng = np.random.default_rng(42)
W = rng.standard_normal((3, 4)).astype(np.float32)  # [out=3, in=4]
b = rng.standard_normal(3).astype(np.float32)  # [3]

# ── ONNX graph: X -> Gemm(transB=1) -> logits -> Softmax -> Y ────────────────
W_init = numpy_helper.from_array(W, name="W")
b_init = numpy_helper.from_array(b, name="b")

gemm = helper.make_node(
    "Gemm",
    inputs=["X", "W", "b"],
    outputs=["logits"],
    transB=1,
)
softmax = helper.make_node(
    "Softmax",
    inputs=["logits"],
    outputs=["Y"],
    axis=1,
)

graph = helper.make_graph(
    [gemm, softmax],
    "iris_bench",
    inputs=[helper.make_tensor_value_info("X", TensorProto.FLOAT, [None, 4])],
    outputs=[helper.make_tensor_value_info("Y", TensorProto.FLOAT, [None, 3])],
    initializer=[W_init, b_init],
)

model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
onnx.checker.check_model(model)

# ── write to fixture path ─────────────────────────────────────────────────────
out_path = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "apps",
    "inference",
    "tests",
    "fixtures",
    "iris.onnx",
)
os.makedirs(os.path.dirname(out_path), exist_ok=True)
onnx.save(model, out_path)
print(f"wrote {out_path}  ({os.path.getsize(out_path)} bytes)")
