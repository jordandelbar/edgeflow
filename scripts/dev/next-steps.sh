#!/usr/bin/env bash
cat <<'EOF'

  cluster ready.

    server      http://localhost:5000
    grafana     http://localhost:3000
    prometheus  http://localhost:9090

  deploy a model (local source - dev mode):
    uv run --with-editable apps/sdk examples/01-quickstart-iris/train.py

  load-test (auto-deploys if needed):
    just bench iris-inference

  tear down:
    just down

EOF
