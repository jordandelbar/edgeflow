"""
Pure-Python tensor wire format codec.

Works inside componentize-py WASM sandboxes (no C extensions needed).

Wire format: [ ndim: u8 | shape: [u32-LE; ndim] | dtype: u8 | data: bytes ]
dtype codes: 1 = f32
"""


def encode_tensor(shape: list, data: bytes) -> bytes:
    """Prepend tensor wire format header to raw float32 bytes.

    Args:
        shape: list of ints, e.g. [1, 4]
        data:  raw f32 little-endian bytes

    The data bytes are passed through unchanged — no re-encoding of floats.
    """
    buf = len(shape).to_bytes(1, "little")
    for dim in shape:
        buf += dim.to_bytes(4, "little")
    buf += (1).to_bytes(1, "little")   # dtype = f32
    return buf + data


def decode_tensor(buf: bytes) -> tuple:
    """Decode tensor wire format → (shape: list[int], values: list[float]).

    Uses a pure-Python IEEE 754 float decoder so it works in WASM.
    Precision is double (Python float), which is sufficient for inference
    outputs (confidence scores, argmax, etc.).
    """
    pos = 0
    ndim = buf[pos]; pos += 1
    shape = []
    for _ in range(ndim):
        dim = int.from_bytes(buf[pos:pos + 4], "little"); pos += 4
        shape.append(dim)
    _dtype = buf[pos]; pos += 1   # must be 1 (f32); skip validation for speed
    values = []
    while pos + 4 <= len(buf):
        values.append(_f32_from_le(buf[pos:pos + 4]))
        pos += 4
    return shape, values


def _f32_from_le(b: bytes) -> float:
    """Decode one IEEE 754 single-precision float from 4 little-endian bytes."""
    bits = int.from_bytes(b, "little")
    sign = -1.0 if (bits >> 31) else 1.0
    exp = (bits >> 23) & 0xFF
    mant = bits & 0x7FFFFF
    if exp == 0:
        # Subnormal
        return sign * (mant / 8388608.0) * 1.1754943508222875e-38   # 2^-126
    if exp == 255:
        return float("nan") if mant else sign * float("inf")
    return sign * (1.0 + mant / 8388608.0) * (2.0 ** (exp - 127))
