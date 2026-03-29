"""
Pipeline: an ordered sequence of standard transform layers.

Standard layers serialise to a JSON config that is logged alongside
standard_pipeline.wasm (pre-compiled Rust, ~150 KB).  Calling
Pipeline.transform() locally runs the same Rust logic via the native
PyO3 extension for fast, guaranteed-accurate local testing.
"""

from __future__ import annotations

import json

from edgeflow.layers import Layer

try:
    from edgeflow import _lib as _native

    _NATIVE_AVAILABLE = True
except ImportError:
    _NATIVE_AVAILABLE = False


class Pipeline:
    def __init__(self, steps: list[Layer]) -> None:
        self.steps = steps

    def to_config(self) -> bytes:
        return json.dumps({"steps": [s.to_config() for s in self.steps]}).encode()

    def transform(self, data: bytes) -> bytes:
        """Run this pipeline locally using the native Rust extension.

        The result is guaranteed to match server-side execution since both
        use the same compiled Rust code.

        Raises:
            RuntimeError: if the native extension is not installed.
                          Build it with: cd python && maturin develop --features python
        """
        if not _NATIVE_AVAILABLE:
            raise RuntimeError(
                "Native Rust extension not available. "
                "Run `maturin develop --features python` in the python/ directory "
                "or `just build-transforms` from the repo root."
            )
        return _native.NativePipeline(self.to_config()).transform(data)
