"""
edgeflow model export helpers.

sklearn_to_onnx   — LogisticRegression, hand-rolled standard ops graph
                    (kept for reference; predates skl2onnx support)
rf_to_onnx        — any skl2onnx-supported classifier via the ml-tools
                    opset; requires ORT backend (tract does not support it)
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


def clf_to_onnx(clf, n_features: int | None = None) -> bytes:
    """Convert any skl2onnx-supported sklearn classifier to ONNX bytes.

    Uses skl2onnx with zipmap=False so probability outputs are plain
    f32 tensors.  The label output is dropped; only the probability
    tensor ([N, n_classes]) is kept, matching the edgeflow pipeline
    convention.

    Requires the ORT inference backend (tract does not support the
    ai.onnx.ml opset that skl2onnx generates).

    Args:
        clf:        any fitted sklearn classifier supported by skl2onnx
        n_features: number of input features; inferred from clf if omitted

    Returns:
        Serialised ONNX model bytes.
    """
    from skl2onnx import convert_sklearn
    from skl2onnx.common.data_types import FloatTensorType

    if n_features is None:
        n_features = clf.n_features_in_

    proto = convert_sklearn(
        clf,
        initial_types=[("X", FloatTensorType([None, n_features]))],
        options={id(clf): {"zipmap": False}},
    )

    # skl2onnx emits two outputs: output[0]=label (int64), output[1]=probabilities (float).
    # Drop the label so the model has a single f32 probability output,
    # matching what the postprocess WASM transform expects.
    prob_output = proto.graph.output[1]
    del proto.graph.output[:]
    proto.graph.output.append(prob_output)

    return proto.SerializeToString()
