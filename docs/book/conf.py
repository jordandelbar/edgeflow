project = "Edgeflow"
author = "Edgeflow contributors"
release = "0.1.0"

extensions = [
    "sphinx_copybutton",
]

html_theme = "furo"
html_title = "Edgeflow"
html_permalinks_icon = "#"

html_static_path = ["_static"]
html_css_files = [
    "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap",
    "custom.css",
]

exclude_patterns = ["_build", ".venv"]
