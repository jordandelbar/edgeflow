# edgeflow-cli

[![Crates.io](https://img.shields.io/crates/v/edgeflow-cli.svg)](https://crates.io/crates/edgeflow-cli)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)

Command-line interface for the [edgeflow](https://github.com/jordandelbar/edgeflow)
inference platform. Manage experiments, registered models, and target
deployments against a running edgeflow server from your terminal.

## Install

```sh
cargo install edgeflow-cli
```

This installs an `edgeflow` binary.

## Quick start

```sh
# Point at your edgeflow server (default: http://localhost:5000)
export EDGEFLOW_SERVER=http://localhost:5000

# List registered models
edgeflow models list

# Inspect a target
edgeflow targets get my-target

# Deploy a model version to a target
edgeflow deploy --model iris --version 3 --target edge-01
```

Run `edgeflow --help` for the full command reference.

## License

Licensed under the [Apache License, Version 2.0](../../LICENSE).
