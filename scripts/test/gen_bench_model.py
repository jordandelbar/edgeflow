"""
Generate a minimal ONNX fixture for inference benchmarks and tests.

The model is a tiny linear classifier (Gemm + Softmax) with:
  input  : float32 [N, 4]   - iris-like features
  output : float32 [N, 3]   - softmax class probabilities

Because the output is already float32 at index 0, it works directly with the
ORT backend without any postprocess WASM.

Usage (from repo root):
    just gen-bench-model
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

# ── large model: [None, 4096] → Gemm → ReLU → [None, 10] ─────────────────────
# Used to benchmark the zero-copy gains on realistic tensor sizes (~16 KB input).
N_IN, N_OUT = 4096, 10
W2 = rng.standard_normal((N_OUT, N_IN)).astype(np.float32)
b2 = rng.standard_normal(N_OUT).astype(np.float32)

W2_init = numpy_helper.from_array(W2, name="W2")
b2_init = numpy_helper.from_array(b2, name="b2")

gemm2 = helper.make_node(
    "Gemm", inputs=["X2", "W2", "b2"], outputs=["logits2"], transB=1
)
softmax2 = helper.make_node("Softmax", inputs=["logits2"], outputs=["Y2"], axis=1)

graph2 = helper.make_graph(
    [gemm2, softmax2],
    "large_bench",
    inputs=[helper.make_tensor_value_info("X2", TensorProto.FLOAT, [None, N_IN])],
    outputs=[helper.make_tensor_value_info("Y2", TensorProto.FLOAT, [None, N_OUT])],
    initializer=[W2_init, b2_init],
)

model2 = helper.make_model(graph2, opset_imports=[helper.make_opsetid("", 17)])
onnx.checker.check_model(model2)

large_path = os.path.join(os.path.dirname(out_path), "large.onnx")
onnx.save(model2, large_path)
print(f"wrote {large_path}  ({os.path.getsize(large_path)} bytes)")
