"""
Iris inference transforms.

These functions live next to the training script — same file you'd use
during training and evaluation. The decorators mark them for WASM
compilation; the functions themselves are plain Python and can be called
directly for local testing.

Imports must be inside the function body: inspect.getsource captures
only the function, and the WASM sandbox has no outer module scope.

Note: componentize-py's WASM sandbox only includes pure-Python stdlib
modules. C extensions (struct, _json, array, etc.) are unavailable.
Use int.from_bytes() / int.to_bytes() for binary encoding instead.
"""

from edgeflow.transforms import postprocess, preprocess


@preprocess
def prepare(raw: bytes) -> bytes:
    """4 × f32 LE → edgeflow tensor wire format [ndim | shape | dtype | data]

    No C-extension imports: uses int.to_bytes() (built-in).
    """
    n = len(raw) // 4
    header = (
        (2).to_bytes(1, "little")    # ndim = 2
        + (1).to_bytes(4, "little")  # shape[0] = 1
        + n.to_bytes(4, "little")    # shape[1] = n
        + (1).to_bytes(1, "little")  # dtype = 1 (f32)
    )
    return header + raw


@postprocess
def interpret(tensor: bytes) -> bytes:
    """edgeflow tensor wire format (3-class probs) → JSON result bytes

    No C-extension imports: uses int.from_bytes() (built-in) to compare
    f32 bit patterns. IEEE 754 positive floats sort identically as u32,
    so argmax via bit comparison is valid for softmax outputs (all ≥ 0).
    """
    LABELS = ["setosa", "versicolor", "virginica"]

    ndim = tensor[0]
    data_offset = 1 + ndim * 4 + 1
    n = (len(tensor) - data_offset) // 4

    # Find argmax by comparing raw u32 bit patterns (valid for 0–1 floats).
    max_idx = 0
    max_bits = 0
    for i in range(n):
        bits = int.from_bytes(tensor[data_offset + i * 4: data_offset + i * 4 + 4], "little")
        if bits > max_bits:
            max_bits = bits
            max_idx = i

    label = LABELS[max_idx] if max_idx < len(LABELS) else "unknown"
    # Manual JSON — avoid json module (also a C extension).
    return ('{"class_id":' + str(max_idx) + ',"label":"' + label + '"}').encode()
