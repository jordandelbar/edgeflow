"""
Tensor wire format codec.

Used on both sides of the edgeflow WASM boundary:
  - Rust host encodes ONNX model output and sends it to the WASM postprocess component.
  - Python WASM component decodes it, transforms it, then re-encodes the result.

Wire format: [ ndim: u8 | dtype: u8 | _pad: u16 | shape: [u32-LE; ndim] | data: bytes ]

dtype codes:
  1 = f32   (float32, 4 bytes/element)
  2 = i32   (int32,   4 bytes/element, signed)
  3 = i64   (int64,   8 bytes/element, signed)
  4 = f64   (float64, 8 bytes/element)
  5 = bool  (boolean, 1 byte/element)
  6 = u8    (uint8,   1 byte/element)

The fixed 4-byte header guarantees data starts at offset 4 + ndim*4,
which is always 4-byte aligned (enabling zero-copy cast in the Rust host
for f32/i32 and with alignment checks for wider types).

Note on float decoding: `struct` is used for f32/f64 because it is part of
CPython's standard C extension set and is included in the componentize-py
WASM build (the entire CPython interpreter is compiled to wasm32-wasi, so
standard-library C modules like _struct are available).  `int.from_bytes`
is used for integers as it is built-in and equally fast.

When numpy is available, `decode_tensor` returns an `ndarray` instead of a
plain list. This avoids ~40 MB of Python object overhead for large tensors
(e.g. 640x640x3 images). Falls back to the list path in environments where
numpy is not installed (WASM).
"""

import struct

try:
    import numpy as np
except ImportError:
    np = None

DTYPE_F32 = 1
DTYPE_I32 = 2
DTYPE_I64 = 3
DTYPE_F64 = 4
DTYPE_BOOL = 5
DTYPE_U8 = 6

_DTYPE_ITEMSIZE = {
    DTYPE_F32: 4,
    DTYPE_I32: 4,
    DTYPE_I64: 8,
    DTYPE_F64: 8,
    DTYPE_BOOL: 1,
    DTYPE_U8: 1,
}

_DTYPE_NUMPY = (
    {
        DTYPE_F32: "<f4",
        DTYPE_I32: "<i4",
        DTYPE_I64: "<i8",
        DTYPE_F64: "<f8",
        DTYPE_BOOL: "?",
        DTYPE_U8: "u1",
    }
    if np is not None
    else {}
)


def encode_tensor(shape: list, data: bytes, dtype: int = DTYPE_F32) -> bytes:
    """Prepend tensor wire format header to raw element bytes.

    Args:
        shape: list of ints, e.g. [1, 4]
        data:  raw little-endian bytes for the chosen dtype
        dtype: one of the DTYPE_* constants (default DTYPE_F32)

    The data bytes are passed through unchanged - no re-encoding of values.
    """
    if dtype not in _DTYPE_ITEMSIZE:
        raise ValueError(f"unknown dtype code: {dtype}")
    expected = 1
    for dim in shape:
        expected *= dim
    expected *= _DTYPE_ITEMSIZE[dtype]
    if len(data) != expected:
        raise ValueError(
            f"encode_tensor: shape {shape} with dtype {dtype} expects "
            f"{expected} bytes of data, got {len(data)}"
        )
    buf = len(shape).to_bytes(1, "little")  # ndim
    buf += dtype.to_bytes(1, "little")  # dtype code
    buf += b"\x00\x00"  # padding
    for dim in shape:
        buf += dim.to_bytes(4, "little")
    return buf + data


def decode_tensor(buf: bytes) -> tuple:
    """Decode tensor wire format → (shape, values).

    When numpy is available, returns (list[int], ndarray).
    Otherwise returns (list[int], list) with element type matching dtype:
      f32/f64  → list[float]
      i32/i64  → list[int]
      bool     → list[bool]
      u8       → list[int]
    """
    if len(buf) < 4:
        raise ValueError(
            f"decode_tensor: buffer too short for header (need 4 bytes, got {len(buf)})"
        )
    ndim = buf[0]
    dtype = buf[1]
    # buf[2:4] = padding
    header_len = 4 + ndim * 4
    if len(buf) < header_len:
        raise ValueError(
            f"decode_tensor: buffer too short for {ndim}-dim shape "
            f"(need {header_len} bytes, got {len(buf)})"
        )
    pos = 4  # fixed header is always 4 bytes
    shape = []
    for _ in range(ndim):
        dim = int.from_bytes(buf[pos : pos + 4], "little")
        pos += 4
        shape.append(dim)

    if dtype not in _DTYPE_ITEMSIZE:
        raise ValueError(f"unknown dtype code: {dtype}")

    itemsize = _DTYPE_ITEMSIZE[dtype]
    expected = 1
    for dim in shape:
        expected *= dim
    expected *= itemsize
    data = buf[pos:]
    if len(data) < expected:
        raise ValueError(
            f"decode_tensor: shape {shape} requires {expected} bytes of data "
            f"but buffer has {len(data)}"
        )
    data = data[:expected]

    if np is not None:
        values = np.frombuffer(data, dtype=_DTYPE_NUMPY[dtype]).reshape(shape)
        return shape, values

    if dtype == DTYPE_F32:
        n = expected // 4
        values = list(struct.unpack_from(f"<{n}f", data))
    elif dtype == DTYPE_F64:
        n = expected // 8
        values = list(struct.unpack_from(f"<{n}d", data))
    elif dtype == DTYPE_I32:
        n = expected // 4
        values = list(struct.unpack_from(f"<{n}i", data))
    elif dtype == DTYPE_I64:
        n = expected // 8
        values = list(struct.unpack_from(f"<{n}q", data))
    elif dtype == DTYPE_BOOL:
        values = [bool(b) for b in data]
    elif dtype == DTYPE_U8:
        values = list(data)

    return shape, values
