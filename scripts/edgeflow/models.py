"""
edgeflow model export helpers.

Handles the conversion from fitted sklearn models to ONNX bytes using
only standard ops (MatMul, Add, Softmax) that tract-onnx understands.

Motivation: skl2onnx produces a LinearClassifier node from the ml-tools
opset which tract-onnx does not implement.  This module replaces that
with an equivalent subgraph of standard arithmetic ops.
"""

from __future__ import annotations


def sklearn_to_onnx(clf, n_features: int | None = None) -> bytes:
    """Convert a fitted sklearn LogisticRegression to ONNX bytes.

    The resulting model accepts a [1, n_features] f32 tensor and returns
    a [1, n_classes] f32 probability tensor (softmax output).

    Args:
        clf:        fitted sklearn LogisticRegression instance
        n_features: number of input features; inferred from clf if omitted

    Returns:
        Serialised ONNX model bytes ready to be saved or passed to edgeflow.
    """
    import numpy as np
    import onnx
    import onnx.helper as oh
    import onnx.numpy_helper as onh

    if n_features is None:
        n_features = clf.coef_.shape[1]

    n_classes = len(clf.classes_)

    W = clf.coef_.astype(np.float32)         # (n_classes, n_features)
    b = clf.intercept_.astype(np.float32)    # (n_classes,)

    nodes = [
        oh.make_node("MatMul", ["X", "W"], ["logits_t"]),
        oh.make_node("Add",    ["logits_t", "b"], ["logits"]),
        oh.make_node("Softmax", ["logits"], ["probabilities"], axis=1),
    ]
    graph = oh.make_graph(
        nodes,
        "lr",
        [oh.make_tensor_value_info("X", onnx.TensorProto.FLOAT, [1, n_features])],
        [oh.make_tensor_value_info("probabilities", onnx.TensorProto.FLOAT, [None, n_classes])],
        initializer=[onh.from_array(W.T, name="W"), onh.from_array(b, name="b")],
    )
    model = oh.make_model(graph, opset_imports=[oh.make_opsetid("", 17)])
    return model.SerializeToString()
