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

from hypothesis import given, settings
from hypothesis import strategies as st

from edgeflow.codec import (
    DTYPE_BOOL,
    DTYPE_F32,
    DTYPE_F64,
    DTYPE_I32,
    DTYPE_I64,
    DTYPE_U8,
    _DTYPE_ITEMSIZE,
    decode_tensor,
    encode_tensor,
)


# ── helpers ────────────────────────────────────────────────────────────────────


def _make_buf(arr: np.ndarray, dtype_code: int) -> bytes:
    """Build a wire-format buffer from a numpy array using its raw bytes."""
    shape = list(arr.shape)
    return encode_tensor(shape, arr.tobytes(), dtype=dtype_code)


def _assert_values_equal(values, expected, dtype_code):
    """Compare decoded values (ndarray or list) against expected ndarray."""
    got = np.asarray(values).flatten()
    exp = np.asarray(expected).flatten()
    if dtype_code in (DTYPE_F32, DTYPE_F64):
        # NaN != NaN in IEEE 754, check separately
        for i in range(len(exp)):
            if math.isnan(exp[i]):
                assert math.isnan(got[i]), f"index {i}: expected nan"
            else:
                assert got[i] == pytest.approx(float(exp[i]), rel=1e-6, abs=1e-45)
    else:
        np.testing.assert_array_equal(got, exp)


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
        _assert_values_equal(values, arr, DTYPE_F32)

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
        assert values.size == 8 * 14

    def test_classifier_probabilities(self):
        # typical postprocess input: [1, 2] probability tensor
        probs = np.array([[0.3, 0.7]], dtype=np.float32)
        shape, values = decode_tensor(_make_buf(probs, DTYPE_F32))
        assert shape == [1, 2]
        _assert_values_equal(values, probs, DTYPE_F32)


# ── f64 round-trip ─────────────────────────────────────────────────────────────


class TestF64:
    def test_basic_values(self):
        arr = np.array([1.23456789012345, -9.87654321], dtype=np.float64)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F64))
        assert shape == [2]
        _assert_values_equal(values, arr, DTYPE_F64)

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
        _assert_values_equal(values, arr, DTYPE_I32)

    def test_negative(self):
        arr = np.array([-1, -128, -(2**31)], dtype=np.int32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I32))
        _assert_values_equal(values, arr, DTYPE_I32)

    def test_mixed(self):
        arr = np.array([-5, 0, 5], dtype=np.int32)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I32))
        _assert_values_equal(values, arr, DTYPE_I32)


# ── i64 round-trip ─────────────────────────────────────────────────────────────


class TestI64:
    def test_large_positive(self):
        arr = np.array([2**62], dtype=np.int64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        _assert_values_equal(values, arr, DTYPE_I64)

    def test_large_negative(self):
        arr = np.array([-(2**62)], dtype=np.int64)
        _, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        _assert_values_equal(values, arr, DTYPE_I64)

    def test_label_output(self):
        # typical classifier label output: [batch] int64 tensor
        arr = np.array([0, 1, 1, 0], dtype=np.int64)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_I64))
        assert shape == [4]
        _assert_values_equal(values, arr, DTYPE_I64)


# ── bool round-trip ────────────────────────────────────────────────────────────


class TestBool:
    def test_true_false(self):
        arr = np.array([True, False, True, True, False], dtype=np.bool_)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert shape == [5]
        _assert_values_equal(values, arr, DTYPE_BOOL)

    def test_all_true(self):
        arr = np.ones(4, dtype=np.bool_)
        _, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert np.all(values)

    def test_all_false(self):
        arr = np.zeros(4, dtype=np.bool_)
        _, values = decode_tensor(_make_buf(arr, DTYPE_BOOL))
        assert not np.any(values)


# ── u8 round-trip ─────────────────────────────────────────────────────────────


class TestU8:
    def test_pixel_values(self):
        arr = np.array([0, 127, 200, 255], dtype=np.uint8)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_U8))
        assert shape == [4]
        np.testing.assert_array_equal(values, arr)

    def test_image_shape(self):
        arr = np.random.randint(0, 256, size=(1, 3, 4, 4), dtype=np.uint8)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_U8))
        assert shape == [1, 3, 4, 4]
        np.testing.assert_array_equal(values.flatten(), arr.flatten())

    def test_single_byte(self):
        arr = np.array([42], dtype=np.uint8)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_U8))
        assert shape == [1]
        assert values[0] == 42

    def test_all_byte_values(self):
        arr = np.arange(256, dtype=np.uint8)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_U8))
        assert shape == [256]
        np.testing.assert_array_equal(values, arr)


# ── edge cases ─────────────────────────────────────────────────────────────────


class TestEdgeCases:
    def test_empty_data(self):
        buf = encode_tensor([0], b"")
        shape, values = decode_tensor(buf)
        assert shape == [0]
        assert np.asarray(values).size == 0

    def test_scalar_shape(self):
        arr = np.array([42.0], dtype=np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [1]
        _assert_values_equal(values, arr, DTYPE_F32)

    def test_3d_shape(self):
        arr = np.ones((2, 3, 4), dtype=np.float32)
        shape, values = decode_tensor(_make_buf(arr, DTYPE_F32))
        assert shape == [2, 3, 4]
        assert values.size == 24

    def test_unknown_dtype_encode_raises(self):
        with pytest.raises(ValueError, match="unknown dtype code"):
            encode_tensor([1], b"\x00" * 4, dtype=99)

    def test_unknown_dtype_decode_raises(self):
        # Hand-craft a buffer with dtype code 99
        buf = b"\x01\x63\x00\x00" + b"\x01\x00\x00\x00" + b"\x00" * 4
        with pytest.raises(ValueError, match="unknown dtype code"):
            decode_tensor(buf)

    def test_encode_decode_preserves_byte_order(self):
        # encode a known f32 value and check the raw bytes match struct.pack
        val = 3.14
        raw = struct.pack("<f", val)
        buf = encode_tensor([1], raw, dtype=DTYPE_F32)
        _, values = decode_tensor(buf)
        assert values[0] == pytest.approx(val, rel=1e-6)


# ── input validation ─────────────────────────────────────────────────────────


class TestValidation:
    def test_decode_empty_buffer(self):
        with pytest.raises(ValueError, match="buffer too short for header"):
            decode_tensor(b"")

    def test_decode_truncated_header(self):
        with pytest.raises(ValueError, match="buffer too short for header"):
            decode_tensor(b"\x01")

    def test_decode_truncated_shape(self):
        # claims 2 dims but only has space for 1
        buf = b"\x02\x01\x00\x00" + b"\x03\x00\x00\x00"
        with pytest.raises(ValueError, match="buffer too short for.*shape"):
            decode_tensor(buf)

    def test_decode_truncated_data(self):
        buf = encode_tensor([1, 4], b"\x00" * 16)
        with pytest.raises(ValueError, match="requires.*bytes of data"):
            decode_tensor(buf[:-4])

    def test_encode_shape_data_mismatch(self):
        with pytest.raises(ValueError, match="expects.*bytes of data"):
            encode_tensor([2, 3], b"\x00" * 4)

    def test_encode_unknown_dtype(self):
        with pytest.raises(ValueError, match="unknown dtype code"):
            encode_tensor([1], b"\x00" * 4, dtype=99)


# ── hypothesis fuzz tests ────────────────────────────────────────────────────


def _product(shape):
    r = 1
    for d in shape:
        r *= d
    return r


# dtype code -> (numpy dtype, element strategy)
_FUZZ_DTYPES = {
    DTYPE_F32: (np.float32, st.floats(width=32, allow_nan=True, allow_infinity=True)),
    DTYPE_F64: (np.float64, st.floats(width=64, allow_nan=True, allow_infinity=True)),
    DTYPE_I32: (np.int32, st.integers(min_value=-(2**31), max_value=2**31 - 1)),
    DTYPE_I64: (np.int64, st.integers(min_value=-(2**63), max_value=2**63 - 1)),
    DTYPE_BOOL: (np.bool_, st.booleans()),
    DTYPE_U8: (np.uint8, st.integers(min_value=0, max_value=255)),
}


@st.composite
def _tensor(draw):
    """Strategy that produces (dtype_code, shape, numpy_array) triples."""
    dtype_code = draw(st.sampled_from(list(_FUZZ_DTYPES.keys())))
    np_dtype, elem_st = _FUZZ_DTYPES[dtype_code]
    # 0-4 dimensions, each 0-8 (capped to keep total elements under hypothesis limit)
    shape = draw(
        st.lists(st.integers(min_value=0, max_value=8), min_size=0, max_size=4)
    )
    n = _product(shape)
    if n == 0:
        arr = np.array([], dtype=np_dtype).reshape(shape)
    else:
        elements = draw(st.lists(elem_st, min_size=n, max_size=n))
        arr = np.array(elements, dtype=np_dtype).reshape(shape)
    return dtype_code, shape, arr


class TestFuzz:
    @given(data=_tensor())
    @settings(max_examples=500)
    def test_round_trip(self, data):
        """encode then decode must recover shape and values for any valid tensor."""
        dtype_code, shape, arr = data
        buf = encode_tensor(shape, arr.tobytes(), dtype=dtype_code)
        dec_shape, dec_values = decode_tensor(buf)

        assert dec_shape == shape

        if arr.size == 0:
            assert np.asarray(dec_values).size == 0
            return

        _assert_values_equal(dec_values, arr, dtype_code)

    @given(garbage=st.binary(min_size=0, max_size=200))
    @settings(max_examples=500)
    def test_garbage_does_not_crash(self, garbage):
        """Random bytes must either decode or raise ValueError, never crash."""
        try:
            decode_tensor(garbage)
        except ValueError:
            pass

    @given(
        dtype_code=st.sampled_from(list(_DTYPE_ITEMSIZE.keys())),
        shape=st.lists(st.integers(min_value=0, max_value=10), min_size=0, max_size=4),
        extra=st.integers(min_value=1, max_value=16),
    )
    @settings(max_examples=200)
    def test_encode_rejects_wrong_length(self, dtype_code, shape, extra):
        """encode_tensor must reject data whose length doesn't match shape."""
        n_elements = _product(shape)
        correct_len = n_elements * _DTYPE_ITEMSIZE[dtype_code]
        wrong_len = correct_len + extra
        with pytest.raises(ValueError, match="expects.*bytes of data"):
            encode_tensor(shape, b"\x00" * wrong_len, dtype=dtype_code)
