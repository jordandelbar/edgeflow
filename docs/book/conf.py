import tomllib
from pathlib import Path

project = "Edgeflow"
author = "Edgeflow contributors"

_workspace_toml = Path(__file__).resolve().parents[2] / "Cargo.toml"
release = tomllib.loads(_workspace_toml.read_text())["workspace"]["package"]["version"]

extensions = [
    "sphinx_copybutton",
]

html_theme = "furo"
html_title = "Edgeflow"
html_permalinks_icon = "#"

# Pygments themes - vivid stock pairs that read well in both modes.
pygments_style = "xcode"
pygments_dark_style = "monokai"

html_static_path = ["_static"]
html_css_files = [
    "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap",
    "custom.css",
]

exclude_patterns = ["_build", ".venv"]
