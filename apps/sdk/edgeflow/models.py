"""edgeflow model utilities.

Two flavours of helpers live here:

**Training-time exporters** (local, no server):
    clf_to_onnx           - unified export for sklearn / XGBoost / LightGBM /
                            CatBoost classifiers; detects framework
    sklearn_to_onnx       - hand-rolled LogisticRegression export, kept for
                            reference (predates skl2onnx support)
    read_onnx_input_shape - read n_features from a float[N, k] ONNX input

**Registry ops** (REST, mirror ``edgeflow models`` CLI subcommand):
    list                  - list registered models
    versions              - list versions of a registered model
    register              - register a run as a model version
    stage                 - transition a model version to a stage
    delete                - delete a registered model
    delete_version        - delete a specific model version
"""

from __future__ import annotations

from edgeflow._internal import client
from edgeflow._types import RegisteredModel, registered_model_from_dict
from edgeflow.deploy import ModelVersion, register


# ── Registry ops (REST) ──────────────────────────────────────────────────────


def list(*, server: str | None = None) -> list[RegisteredModel]:
    """List all registered models. Mirrors ``edgeflow models list``."""
    res = client(server).list_registered_models()
    return [registered_model_from_dict(m) for m in res.get("registered_models", [])]


def versions(name: str, *, server: str | None = None) -> list[ModelVersion]:
    """List versions of a registered model. Mirrors ``edgeflow models versions``."""
    res = client(server).list_model_versions(name)
    return [
        ModelVersion(
            name=v["name"],
            version=str(v["version"]),
            run_id=v.get("run_id"),
            current_stage=v.get("current_stage", "None"),
        )
        for v in res.get("model_versions", [])
    ]


def stage(
    name: str, version: str, stage: str, *, server: str | None = None
) -> ModelVersion:
    """Transition a model version to a stage. Mirrors ``edgeflow models stage``."""
    res = client(server).transition_stage(name, version, stage)
    v = res.get("model_version", res)
    return ModelVersion(
        name=v["name"],
        version=str(v["version"]),
        run_id=v.get("run_id"),
        current_stage=v.get("current_stage", "None"),
    )


def delete(name: str, *, server: str | None = None) -> None:
    """Delete a registered model. Mirrors ``edgeflow models delete``."""
    client(server).delete_registered_model(name)


def delete_version(name: str, version: str, *, server: str | None = None) -> None:
    """Delete a specific model version. Mirrors ``edgeflow models delete-version``."""
    client(server).delete_model_version(name, version)


# Re-export ``register`` (lives in deploy.py for historical reasons - it
# was the first registry op to land). Both ``edgeflow.register`` and
# ``edgeflow.models.register`` resolve to the same function.
__all__ = [
    "list",
    "versions",
    "register",
    "stage",
    "delete",
    "delete_version",
    "clf_to_onnx",
    "sklearn_to_onnx",
    "read_onnx_input_shape",
]


# ── Training-time exporters (local) ──────────────────────────────────────────


def clf_to_onnx(clf, n_features: int | None = None) -> bytes:
    """Convert a fitted classifier to ONNX bytes.

    Detects the source framework (sklearn, XGBoost, LightGBM, CatBoost) from
    the class module and routes to the appropriate export path.  All paths
    produce a single f32 probability tensor output ([N, n_classes]) matching
    the edgeflow ClassifierOutput convention.

    Args:
        clf:        fitted classifier (XGBClassifier, LGBMClassifier,
                    CatBoostClassifier, or any skl2onnx-supported sklearn clf)
        n_features: number of input features; inferred from clf if omitted

    Returns:
        Serialised ONNX model bytes.
    """
    module = type(clf).__module__.split(".")[0]
    if module == "xgboost":
        return _xgb_clf_to_onnx(clf, n_features)
    if module == "lightgbm":
        return _lgbm_clf_to_onnx(clf, n_features)
    if module == "catboost":
        return _catboost_clf_to_onnx(clf)
    return _skl_clf_to_onnx(clf, n_features)


# ── framework-specific helpers ─────────────────────────────────────────────────


def _skl_clf_to_onnx(clf, n_features: int | None = None) -> bytes:
    """skl2onnx path for native sklearn classifiers."""
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
    # Drop the label so the model has a single f32 probability output.
    prob_output = proto.graph.output[1]
    del proto.graph.output[:]
    proto.graph.output.append(prob_output)

    return proto.SerializeToString()


def _xgb_clf_to_onnx(clf, n_features: int | None = None) -> bytes:
    """ONNX export for XGBClassifier via onnxmltools-registered skl2onnx converter."""
    import xgboost

    try:
        from onnxmltools.convert.xgboost.operator_converters.XGBoost import (
            convert_xgboost,
        )
    except ImportError as exc:
        raise ImportError(
            "onnxmltools is required for XGBoost ONNX export: pip install edgeflow[xgboost]"
        ) from exc

    from skl2onnx import update_registered_converter
    from skl2onnx.common.shape_calculator import (
        calculate_linear_classifier_output_shapes,
    )

    update_registered_converter(
        xgboost.XGBClassifier,
        "XGBoostXGBClassifier",
        calculate_linear_classifier_output_shapes,
        convert_xgboost,
        options={"nocl": [True, False], "zipmap": [True, False, "columns"]},
    )

    if n_features is None:
        n_features = clf.n_features_in_

    from skl2onnx import convert_sklearn
    from skl2onnx.common.data_types import FloatTensorType

    proto = convert_sklearn(
        clf,
        initial_types=[("X", FloatTensorType([None, n_features]))],
        target_opset={"": 12, "ai.onnx.ml": 2},
        options={id(clf): {"zipmap": False, "nocl": False}},
    )

    prob_output = proto.graph.output[1]
    del proto.graph.output[:]
    proto.graph.output.append(prob_output)

    return proto.SerializeToString()


def _lgbm_clf_to_onnx(clf, n_features: int | None = None) -> bytes:
    """ONNX export for LGBMClassifier via onnxmltools."""
    try:
        from onnxmltools.convert import convert_lightgbm
        from onnxmltools.convert.common.data_types import FloatTensorType as OmlFloat
    except ImportError as exc:
        raise ImportError(
            "onnxmltools is required for LightGBM ONNX export: pip install edgeflow[lightgbm]"
        ) from exc

    if n_features is None:
        n_features = clf.n_features_in_

    proto = convert_lightgbm(
        clf,
        initial_types=[("X", OmlFloat([None, n_features]))],
        target_opset=12,
        zipmap=False,
    )

    if len(proto.graph.output) > 1:
        prob_output = proto.graph.output[1]
        del proto.graph.output[:]
        proto.graph.output.append(prob_output)

    return proto.SerializeToString()


def _catboost_clf_to_onnx(clf) -> bytes:
    """ONNX export for CatBoostClassifier.

    CatBoost's native ONNX export always inserts a ZipMap node that turns the
    raw float probability tensor into Sequence<Map<int64, float>>, which ORT
    cannot extract as a tensor.  We remove the ZipMap node and wire its input
    (the raw f32 probability tensor from TreeEnsembleClassifier) directly as
    the graph output.
    """
    import os
    import tempfile

    import onnx
    import onnx.helper as oh

    with tempfile.NamedTemporaryFile(suffix=".onnx", delete=False) as f:
        tmp_path = f.name
    try:
        clf.save_model(tmp_path, format="onnx")
        proto = onnx.load(tmp_path)
    finally:
        os.unlink(tmp_path)

    # Find the ZipMap node and record its input (raw probability tensor name).
    zipmap_input = None
    nodes_to_keep = []
    for node in proto.graph.node:
        if node.op_type == "ZipMap" and node.domain == "ai.onnx.ml":
            zipmap_input = node.input[0]
        else:
            nodes_to_keep.append(node)

    if zipmap_input is None:
        raise RuntimeError(
            "expected a ZipMap node in CatBoost ONNX graph but found none; "
            "the export format may have changed"
        )

    # Determine output shape from TreeEnsembleClassifier attributes.
    n_classes = None
    for node in proto.graph.node:
        if node.op_type == "TreeEnsembleClassifier":
            for attr in node.attribute:
                if attr.name == "classlabels_int64s":
                    n_classes = len(attr.ints)
                    break
            break

    # Replace graph outputs: drop label + ZipMap probabilities, add raw tensor.
    prob_output = oh.make_tensor_value_info(
        zipmap_input, onnx.TensorProto.FLOAT, [None, n_classes]
    )
    del proto.graph.node[:]
    proto.graph.node.extend(nodes_to_keep)
    del proto.graph.output[:]
    proto.graph.output.append(prob_output)

    return proto.SerializeToString()


# ── reference implementation ───────────────────────────────────────────────────


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

    W = clf.coef_.astype(np.float32)  # (n_classes, n_features)
    b = clf.intercept_.astype(np.float32)  # (n_classes,)

    nodes = [
        oh.make_node("MatMul", ["X", "W"], ["logits_t"]),
        oh.make_node("Add", ["logits_t", "b"], ["logits"]),
        oh.make_node("Softmax", ["logits"], ["probabilities"], axis=1),
    ]
    graph = oh.make_graph(
        nodes,
        "lr",
        [oh.make_tensor_value_info("X", onnx.TensorProto.FLOAT, [1, n_features])],
        [
            oh.make_tensor_value_info(
                "probabilities", onnx.TensorProto.FLOAT, [None, n_classes]
            )
        ],
        initializer=[onh.from_array(W.T, name="W"), onh.from_array(b, name="b")],
    )
    model = oh.make_model(graph, opset_imports=[oh.make_opsetid("", 17)])
    return model.SerializeToString()


# ── ONNX graph inspection ──────────────────────────────────────────────────────


def read_onnx_named_inputs(model_bytes: bytes) -> list[dict] | None:
    """Return ordered input specs for models with multiple or mixed-type inputs.

    Returns a list of ``{"name": str, "type": "float"|"string"}`` dicts when
    the ONNX model has more than one input - i.e. a full sklearn pipeline
    exported via skl2onnx with per-column tensors.

    Returns ``None`` for models with a single float32 tensor input so the
    existing ``FloatBytesToTensor`` auto-injection path continues to work.

    Args:
        model_bytes: serialised ONNX model bytes.

    Returns:
        List of field specs, or None for single-tensor models.
    """
    import onnx

    proto = onnx.ModelProto()
    proto.ParseFromString(model_bytes)

    initializer_names = {i.name for i in proto.graph.initializer}
    true_inputs = [i for i in proto.graph.input if i.name not in initializer_names]

    if len(true_inputs) <= 1:
        return None

    specs = []
    for inp in true_inputs:
        tensor_type = inp.type.tensor_type
        if tensor_type.elem_type == onnx.TensorProto.STRING:
            dtype = "string"
        else:
            dtype = "float"
        specs.append({"name": inp.name, "type": dtype})

    return specs


def read_onnx_input_shape(model_bytes: bytes) -> int | None:
    """Return n_features if the ONNX model has a single float[batch, n_features] input.

    Returns None for models with multi-dimensional inputs (e.g. image models),
    multiple inputs, non-float inputs, or dynamic feature dimensions.

    Args:
        model_bytes: serialised ONNX model bytes.

    Returns:
        n_features as an int, or None if the input shape cannot be determined.
    """
    import onnx

    proto = onnx.ModelProto()
    proto.ParseFromString(model_bytes)

    # Exclude initializers (weights/biases) - only real graph inputs matter.
    initializer_names = {i.name for i in proto.graph.initializer}
    true_inputs = [i for i in proto.graph.input if i.name not in initializer_names]

    if len(true_inputs) != 1:
        return None

    tensor_type = true_inputs[0].type.tensor_type
    if tensor_type.elem_type != onnx.TensorProto.FLOAT:
        return None

    shape = tensor_type.shape
    if shape is None or len(shape.dim) != 2:
        return None

    features_dim = shape.dim[1]
    if features_dim.HasField("dim_value") and features_dim.dim_value > 0:
        return features_dim.dim_value

    return None
