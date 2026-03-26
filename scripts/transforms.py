"""
Iris inference transforms.

These functions live next to the training script — same file you'd use
during training and evaluation. The decorators mark them for WASM
compilation; the functions themselves are plain Python and can be called
directly for local testing.

edgeflow.codec is a pure-Python library bundled into the WASM component
at compile time, so encode_tensor / decode_tensor work inside the sandbox.
"""

from edgeflow.codec import decode_tensor, encode_tensor
from edgeflow.transforms import postprocess, preprocess


@preprocess
def prepare(raw: bytes) -> bytes:
    """4 × f32 LE → edgeflow tensor wire format"""
    n = len(raw) // 4
    return encode_tensor([1, n], raw)


@postprocess
def interpret(tensor: bytes) -> bytes:
    """edgeflow tensor wire format (3-class probs) → JSON result bytes"""
    LABELS = ["setosa", "versicolor", "virginica"]

    _, probs = decode_tensor(tensor)
    class_id = probs.index(max(probs))
    label = LABELS[class_id] if class_id < len(LABELS) else "unknown"
    confidence = round(probs[class_id], 4)
    # json module is not bundled by componentize-py; construct manually.
    return f'{{"class_id":{class_id},"label":"{label}","confidence":{confidence}}}'.encode()
