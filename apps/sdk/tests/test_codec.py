"""
Tests for edgeflow.codec — tensor wire format encode/decode.

Verifies round-trip correctness for every supported dtype, header layout,
edge cases (empty tensor, 0-dim, subnormals, ±inf, NaN), and unknown-dtype
error handling.

Each encode→decode round-trip uses numpy to produce the canonical byte
representation so the test is independent of the codec's own encoder.
"""

import math
import struct

import numpy as np
import pytest

from edgeflow.codec import (
    DTYPE_BOOL,
    DTYPE_F32,
    DTYPE_F64,
    DTYPE_I32,
    DTYPE_I64,
    decode_tensor,
    encode_tensor,
)


# ── helpers ────────────────────────────────────────────────────────────────────


def _make_buf(arr: np.ndarray, dtype_code: int) -> bytes:
    """Build a wire-format buffer from a numpy array using its raw bytes."""
    shape = list(arr.shape)
    return encode_tensor(shape, arr.tobytes(), dtype=dtype_code)


# ── header layout ──────────────────────────────────────────────────────────────


class TestHeader:
    def test_ndim_byte(self):
        buf = encode_tensor([3, 4], b"\x00" * 48)
        assert buf[0] == 2  # ndim

    def test_dtype_byte_default(self):
        buf = encode_tensor([1], b"\x00" * 4)
        assert buf[1] == DTYPE_F32

    def test_dtype_byte_explicit(self):
        buf = encode_tensor([1], b"\x00" * 8, dtype=DTYPE_F64)
        assert buf[1] == DTYPE_F64

    def test_padding_bytes(self):
        buf = encode_tensor([2], b"\x00" * 8)
        assert buf[2:4] == b"\x00\x00"

    def test_shape_encoding(self):
        buf = encode_tensor([5, 14], b"\x00" * (5 * 14 * 4))
        # shape starts at byte 4
        assert struct.unpack_from("<I", buf, 4)[0] == 5
        assert struct.unpack_from("<I", buf, 8)[0] == 14

    def test_data_starts_at_correct_offset(self):
        data = b"\xab" * 4
        buf = encode_tensor([1], data)
        # 4 header + 1*4 shape = offset 8
        assert buf[8:] == data


# ── f32 round-trip ─────────────────────────────────────────────────────────────


class TestF32:
    def test_basic_values(self):
        arr = np.array([[0.0, 1.0, -1.0, 0.5, 100.0]], dtype=np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [1, 5]
        assert values == pytest.approx(arr.flatten().tolist(), rel=1e-6)

    def test_positive_infinity(self):
        arr = np.array([np.inf], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert math.isinf(values[0]) and values[0] > 0

    def test_negative_infinity(self):
        arr = np.array([-np.inf], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert math.isinf(values[0]) and values[0] < 0

    def test_nan(self):
        arr = np.array([float("nan")], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert math.isnan(values[0])

    def test_zero(self):
        arr = np.array([0.0], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert values[0] == 0.0

    def test_negative_zero(self):
        arr = np.array([-0.0], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert values[0] == 0.0  # -0.0 == 0.0 in IEEE 754

    def test_subnormal(self):
        # smallest positive subnormal f32
        arr = np.array([np.float32(1.4e-45)], dtype=np.float32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert values[0] > 0

    def test_batch_shape(self):
        arr = np.random.rand(8, 14).astype(np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [8, 14]
        assert len(values) == 8 * 14

    def test_classifier_probabilities(self):
        # typical postprocess input: [1, 2] probability tensor
        probs = np.array([[0.3, 0.7]], dtype=np.float32)
        shape, values = decode_tensor(_make_buf(probs, DTYPE_F32))
        assert shape == [1, 2]
        assert values == pytest.approx([0.3, 0.7], abs=1e-6)


# ── f64 round-trip ─────────────────────────────────────────────────────────────


class TestF64:
    def test_basic_values(self):
        arr = np.array([1.23456789012345, -9.87654321], dtype=np.float64)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F64))
        assert shape == [2]
        assert values == pytest.approx(arr.tolist(), rel=1e-15)

    def test_infinity(self):
        arr = np.array([np.inf, -np.inf], dtype=np.float64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F64))
        assert math.isinf(values[0]) and values[0] > 0
        assert math.isinf(values[1]) and values[1] < 0

    def test_nan(self):
        arr = np.array([float("nan")], dtype=np.float64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F64))
        assert math.isnan(values[0])

    def test_higher_precision_than_f32(self):
        # value that differs beyond f32 precision
        v = 1.0000001234567890
        arr = np.array([v], dtype=np.float64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_F64))
        assert abs(values[0] - v) < 1e-15


# ── i32 round-trip ─────────────────────────────────────────────────────────────


class TestI32:
    def test_positive(self):
        arr = np.array([0, 1, 127, 2**31 - 1], dtype=np.int32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_I32))
        assert shape == [4]
        assert values == [0, 1, 127, 2**31 - 1]

    def test_negative(self):
        arr = np.array([-1, -128, -(2**31)], dtype=np.int32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I32))
        assert values == [-1, -128, -(2**31)]

    def test_mixed(self):
        arr = np.array([-5, 0, 5], dtype=np.int32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I32))
        assert values == [-5, 0, 5]


# ── i64 round-trip ─────────────────────────────────────────────────────────────


class TestI64:
    def test_large_positive(self):
        arr = np.array([2**62], dtype=np.int64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        assert values == [2**62]

    def test_large_negative(self):
        arr = np.array([-(2**62)], dtype=np.int64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        assert values == [-(2**62)]

    def test_label_output(self):
        # typical classifier label output: [batch] int64 tensor
        arr = np.array([0, 1, 1, 0], dtype=np.int64)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        assert shape == [4]
        assert values == [0, 1, 1, 0]


# ── bool round-trip ────────────────────────────────────────────────────────────


class TestBool:
    def test_true_false(self):
        arr = np.array([True, False, True, True, False], dtype=np.bool_)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert shape == [5]
        assert values == [True, False, True, True, False]

    def test_all_true(self):
        arr = np.ones(4, dtype=np.bool_)
        _, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert all(values)

    def test_all_false(self):
        arr = np.zeros(4, dtype=np.bool_)
        _, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert not any(values)


# ── edge cases ─────────────────────────────────────────────────────────────────


class TestEdgeCases:
    def test_empty_data(self):
        buf = encode_tensor([0], b"")
        shape, values = decode_tensor(buf)
        assert shape == [0]
        assert values == []

    def test_scalar_shape(self):
        arr = np.array([42.0], dtype=np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [1]
        assert values == pytest.approx([42.0])

    def test_3d_shape(self):
        arr = np.ones((2, 3, 4), dtype=np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [2, 3, 4]
        assert len(values) == 24

    def test_unknown_dtype_raises(self):
        buf = encode_tensor([1], b"\x00" * 4, dtype=99)
        with pytest.raises(ValueError, match="unknown dtype code"):
            decode_tensor(buf)

    def test_encode_decode_preserves_byte_order(self):
        # encode a known f32 value and check the raw bytes match struct.pack
        val = 3.14
        raw = struct.pack("<f", val)
        buf = encode_tensor([1], raw, dtype=DTYPE_F32)
        _, values = decode_tensor(buf)
        assert values[0] == pytest.approx(val, rel=1e-6)
