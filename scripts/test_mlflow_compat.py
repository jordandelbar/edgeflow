"""
End-to-end compatibility test against edgeflow using the official MLflow client.
Requires: pip install mlflow

Usage:
    python scripts/test_mlflow_compat.py [--uri http://localhost:5000]
"""

import argparse
import math
import sys
import time

import mlflow
from mlflow.tracking import MlflowClient

PASS = "\033[32m✓\033[0m"
FAIL = "\033[31m✗\033[0m"

failures = []


def check(label: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  {PASS} {label}")
    else:
        msg = f"{label}" + (f": {detail}" if detail else "")
        print(f"  {FAIL} {msg}")
        failures.append(msg)


def section(title: str) -> None:
    print(f"\n{title}")
    print("─" * len(title))


# ── Experiments ────────────────────────────────────────────────────────────────

def test_experiments(client: MlflowClient) -> str:
    section("Experiments")

    exp_name = f"test-experiment-{int(time.time())}"
    exp_id = client.create_experiment(exp_name)
    check("create_experiment returns an ID", bool(exp_id))

    exp = client.get_experiment(exp_id)
    check("get_experiment by ID", exp.name == exp_name)

    exp_by_name = client.get_experiment_by_name(exp_name)
    check("get_experiment_by_name", exp_by_name is not None and exp_by_name.experiment_id == exp_id)

    all_exps = client.search_experiments()
    ids = [e.experiment_id for e in all_exps]
    check("search_experiments includes new experiment", exp_id in ids)

    client.set_experiment_tag(exp_id, "env", "test")
    exp_tagged = client.get_experiment(exp_id)
    tag_val = exp_tagged.tags.get("env")
    check("set_experiment_tag", tag_val == "test", f"got {tag_val!r}")

    new_name = exp_name + "-renamed"
    client.rename_experiment(exp_id, new_name)
    check("rename_experiment", client.get_experiment(exp_id).name == new_name)

    return exp_id


# ── Runs ───────────────────────────────────────────────────────────────────────

def test_runs(client: MlflowClient, exp_id: str) -> str:
    section("Runs")

    run = client.create_run(exp_id, run_name="test-run")
    run_id = run.info.run_id
    check("create_run returns a run_id", bool(run_id))
    check("initial status is RUNNING", run.info.status == "RUNNING")

    fetched = client.get_run(run_id)
    check("get_run", fetched.info.run_id == run_id)

    results = client.search_runs([exp_id])
    check("search_runs finds the run", any(r.info.run_id == run_id for r in results))

    client.set_tag(run_id, "model", "linear")
    fetched = client.get_run(run_id)
    check("set_tag", fetched.data.tags.get("model") == "linear")

    return run_id


# ── Params & Metrics ───────────────────────────────────────────────────────────

def test_logging(client: MlflowClient, run_id: str) -> None:
    section("Params & Metrics")

    client.log_param(run_id, "lr", "0.01")
    client.log_param(run_id, "epochs", "10")
    run = client.get_run(run_id)
    params = run.data.params  # dict {key: value}
    check("log_param (lr)", params.get("lr") == "0.01")
    check("log_param (epochs)", params.get("epochs") == "10")

    now = int(time.time() * 1000)
    client.log_metric(run_id, "loss", 1.0, timestamp=now, step=0)
    client.log_metric(run_id, "loss", 0.5, timestamp=now + 1000, step=1)
    client.log_metric(run_id, "loss", 0.25, timestamp=now + 2000, step=2)
    history = client.get_metric_history(run_id, "loss")
    check("log_metric / get_metric_history length", len(history) == 3, f"got {len(history)}")
    check("metric values decrease", history[0].value > history[-1].value)

    client.log_batch(
        run_id,
        metrics=[
            mlflow.entities.Metric("acc", 0.8, now, 0),
            mlflow.entities.Metric("acc", 0.9, now + 1000, 1),
        ],
        params=[mlflow.entities.Param("optimizer", "adam")],
        tags=[mlflow.entities.RunTag("batch_test", "true")],
    )
    run = client.get_run(run_id)
    params = run.data.params  # dict {key: value}
    acc_history = client.get_metric_history(run_id, "acc")
    check("log_batch metrics", len(acc_history) == 2, f"got {len(acc_history)}")
    check("log_batch params", params.get("optimizer") == "adam")
    check("log_batch tags", run.data.tags.get("batch_test") == "true")


# ── Artifacts ─────────────────────────────────────────────────────────────────

def test_artifacts(client: MlflowClient, run_id: str) -> None:
    section("Artifacts")

    # Upload a small file via the high-level mlflow API
    import tempfile, os
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write("edge model weights placeholder\n")
        tmp_path = f.name

    try:
        mlflow.log_artifact(tmp_path, run_id=run_id)
        artifacts = client.list_artifacts(run_id)
        check("list_artifacts after upload", len(artifacts) >= 1, f"got {len(artifacts)}")
    except Exception as e:
        check("artifact upload", False, str(e))
    finally:
        os.unlink(tmp_path)


# ── Run lifecycle ──────────────────────────────────────────────────────────────

def test_lifecycle(client: MlflowClient, run_id: str, exp_id: str) -> None:
    section("Run lifecycle")

    client.set_terminated(run_id, status="FINISHED")
    run = client.get_run(run_id)
    check("set_terminated FINISHED", run.info.status == "FINISHED")

    client.delete_run(run_id)
    run = client.get_run(run_id)
    check("delete_run sets lifecycle_stage=deleted", run.info.lifecycle_stage == "deleted")

    client.restore_run(run_id)
    run = client.get_run(run_id)
    check("restore_run", run.info.lifecycle_stage == "active")

    client.delete_experiment(exp_id)
    exp = client.get_experiment(exp_id)
    check("delete_experiment", exp.lifecycle_stage == "deleted")

    client.restore_experiment(exp_id)
    exp = client.get_experiment(exp_id)
    check("restore_experiment", exp.lifecycle_stage == "active")


# ── Fluent API smoke test ──────────────────────────────────────────────────────

def test_fluent_api(tracking_uri: str) -> None:
    section("Fluent API (mlflow.start_run)")

    mlflow.set_tracking_uri(tracking_uri)
    mlflow.set_experiment("fluent-test")

    with mlflow.start_run(run_name="fluent-run") as run:
        mlflow.log_param("alpha", 0.5)
        for step in range(5):
            mlflow.log_metric("mse", 1.0 / (step + 1), step=step)
        mlflow.set_tag("source", "fluent")

    check("fluent run completes without error", True)

    client = MlflowClient(tracking_uri)
    finished = client.get_run(run.info.run_id)
    check("fluent run status is FINISHED", finished.info.status == "FINISHED")
    check("fluent param logged", finished.data.params.get("alpha") == "0.5")  # data.params is a dict


# ── Main ───────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--uri", default="http://localhost:5000", help="edgeflow server URI")
    args = parser.parse_args()

    print(f"Testing edgeflow at {args.uri}\n")

    mlflow.set_tracking_uri(args.uri)
    client = MlflowClient(args.uri)

    exp_id = test_experiments(client)
    run_id = test_runs(client, exp_id)
    test_logging(client, run_id)
    test_artifacts(client, run_id)
    test_lifecycle(client, run_id, exp_id)
    test_fluent_api(args.uri)

    print()
    if failures:
        print(f"\033[31m{len(failures)} test(s) failed:\033[0m")
        for f in failures:
            print(f"  • {f}")
        sys.exit(1)
    else:
        print(f"\033[32mAll tests passed.\033[0m")


if __name__ == "__main__":
    main()
