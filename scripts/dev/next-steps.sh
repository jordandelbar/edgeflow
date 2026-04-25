#!/usr/bin/env bash
cat <<'EOF'

  cluster ready.

    server      http://localhost:5000
    grafana     http://localhost:3000
    prometheus  http://localhost:9090

  deploy a model:
    cd examples/01-quickstart-iris && uv run python train.py

  tear down:
    just down

EOF
