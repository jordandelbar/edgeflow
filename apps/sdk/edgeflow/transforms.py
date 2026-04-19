"""
Minimal edgeflow transforms SDK (PoC).

Users decorate their transform functions with @preprocess and @postprocess.
The SDK compiles them to WASM components via componentize-py when
compile_transforms() is called.

Full vision (ADR-002): this SDK ships as a proper Python package with
`edgeflow model push --transforms transforms.py` CLI integration.
"""

import ast
import inspect
import shutil
import subprocess
import tempfile
from pathlib import Path

_registry: dict[str, object] = {}


def preprocess(fn):
    """Mark a function as the model's input transform.

    The function receives raw bytes from the caller and must return bytes
    in the edgeflow tensor wire format understood by edgeflow-inference.
    """
    _registry["preprocess"] = fn
    return fn


def postprocess(fn):
    """Mark a function as the model's output transform.

    The function receives tensor wire format bytes from edgeflow-inference
    and must return raw bytes sent back to the caller.
    """
    _registry["postprocess"] = fn
    return fn


def _extract_codec_imports(fn) -> str:
    """Return any `from edgeflow.codec import ...` lines from fn's module.

    This lets users write top-level imports in their transforms file
    (natural Python) rather than repeating them inside every function body.
    Only edgeflow.codec imports are forwarded - third-party imports like
    sklearn / numpy won't be available inside the WASM sandbox anyway.
    """
    module = inspect.getmodule(fn)
    if module is None:
        return ""
    try:
        tree = ast.parse(inspect.getsource(module))
    except (OSError, TypeError):
        return ""
    lines = []
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.module == "edgeflow.codec":
            lines.append(ast.unparse(node))
    return "\n".join(lines)


def compile_transforms(wit_dir: Path, output_dir: Path) -> dict[str, Path]:
    """Compile registered transforms to WASM components via componentize-py.

    Uses inspect.getsource to extract each decorated function, injects
    `transform = <fn_name>` so componentize-py can find the WIT export,
    then calls componentize-py to produce the .wasm artifact.

    Imports must be inside the transform function body so they are
    captured by inspect.getsource and available inside the WASM sandbox.

    Returns a dict {"preprocess": Path, "postprocess": Path}.
    """
    output_dir = Path(output_dir)
    wit_dir = Path(wit_dir)
    compiled: dict[str, Path] = {}

    # Copy edgeflow package into output_dir so componentize-py can import it
    # (componentize-py runs with cwd=output_dir, which Python adds to sys.path).
    edgeflow_dst = output_dir / "edgeflow"
    if not edgeflow_dst.exists():
        shutil.copytree(Path(__file__).parent, edgeflow_dst)

    for role in ("preprocess", "postprocess"):
        fn = _registry.get(role)
        if fn is None:
            raise RuntimeError(
                f"No @{role} function registered. "
                f"Did you import your transforms module before calling compile_transforms()?"
            )

        # Collect `from edgeflow.codec import ...` lines from the function's
        # module so users can write top-level imports in their transforms file
        # instead of repeating them inside every function body.
        codec_imports = _extract_codec_imports(fn)

        # Extract function source, strip decorator lines.
        fn_source = inspect.getsource(fn)
        fn_source = "\n".join(
            line for line in fn_source.splitlines() if not line.startswith("@")
        )
        # componentize-py 0.21+ looks for a WitWorld class with a transform method.
        import_header = (codec_imports + "\n\n") if codec_imports else ""
        wasm_source = (
            f"{import_header}"
            f"{fn_source}\n\n"
            f"class WitWorld:\n"
            f"    def transform(self, input: bytes) -> bytes:\n"
            f"        return {fn.__name__}(input)\n"
        )

        wasm_out = output_dir / f"{role}.wasm"

        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".py", delete=False, dir=output_dir
        ) as f:
            f.write(wasm_source)
            tmp_py = Path(f.name)

        try:
            subprocess.run(
                [
                    "componentize-py",
                    "--wit-path",
                    str(wit_dir),
                    "--world",
                    "transform",
                    "componentize",
                    tmp_py.stem,
                    "--output",
                    str(wasm_out),
                ],
                cwd=output_dir,
                check=True,
            )
        finally:
            tmp_py.unlink(missing_ok=True)

        size_kb = wasm_out.stat().st_size // 1024
        print(f"  {role}.wasm ({size_kb} KB)")
        compiled[role] = wasm_out

    return compiled
