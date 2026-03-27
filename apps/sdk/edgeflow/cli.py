"""edgeflow CLI — deploy runs to inference targets and watch state transitions."""

import sys
import time
from typing import Optional

import typer
import requests

app = typer.Typer(help="edgeflow CLI")

TERMINAL_STATES = {"healthy", "failed", "superseded"}


@app.command()
def deploy(
    run_id: str = typer.Argument(..., help="MLflow run ID to deploy"),
    target: str = typer.Argument(..., help="Inference target name (e.g. iris-inference)"),
    server: str = typer.Option("http://localhost:5000", help="edgeflow server URL"),
    timeout: int = typer.Option(300, help="Seconds to wait for the deployment to become healthy"),
):
    """Deploy a run to an inference target and wait for it to become healthy."""
    typer.echo(f"Deploying run {run_id} → target '{target}'")

    # POST /api/v1/deployments
    resp = requests.post(
        f"{server}/api/v1/deployments",
        json={"run_id": run_id, "target": target},
        timeout=10,
    )
    if not resp.ok:
        typer.secho(f"Failed to create deployment: {resp.status_code} {resp.text}", fg=typer.colors.RED)
        raise typer.Exit(1)

    deployment = resp.json()["deployment"]
    deployment_id = deployment["deployment_id"]
    typer.echo(f"Deployment {deployment_id} created — state: {deployment['state']}")

    # Poll until terminal state or timeout.
    deadline = time.monotonic() + timeout
    last_state = deployment["state"]

    while time.monotonic() < deadline:
        time.sleep(2)

        resp = requests.get(
            f"{server}/api/v1/deployments/{deployment_id}",
            timeout=10,
        )
        if not resp.ok:
            typer.secho(f"Failed to poll deployment: {resp.status_code}", fg=typer.colors.RED)
            raise typer.Exit(1)

        current_state = resp.json()["deployment"]["state"]

        if current_state != last_state:
            typer.echo(f"  {last_state} → {current_state}")
            last_state = current_state

        if current_state in TERMINAL_STATES:
            break

    if last_state == "healthy":
        typer.secho(f"Deployment {deployment_id} is healthy.", fg=typer.colors.GREEN)
    elif last_state in TERMINAL_STATES:
        typer.secho(f"Deployment {deployment_id} ended in state: {last_state}", fg=typer.colors.RED)
        raise typer.Exit(1)
    else:
        typer.secho(
            f"Timed out after {timeout}s — deployment still in state: {last_state}",
            fg=typer.colors.RED,
        )
        raise typer.Exit(1)


@app.command()
def status(
    target: str = typer.Argument(..., help="Inference target name"),
    server: str = typer.Option("http://localhost:5000", help="edgeflow server URL"),
):
    """Show the latest deployment state for a target."""
    resp = requests.get(
        f"{server}/api/v1/deployments/latest",
        params={"target": target},
        timeout=10,
    )
    if not resp.ok:
        typer.secho(f"No deployment found for target '{target}'.", fg=typer.colors.YELLOW)
        raise typer.Exit(1)

    d = resp.json()["deployment"]
    state = d["state"]
    color = typer.colors.GREEN if state == "healthy" else (
        typer.colors.RED if state in {"failed"} else typer.colors.YELLOW
    )
    typer.secho(
        f"target={target}  deployment_id={d['deployment_id']}  run_id={d['run_id']}  state={state}",
        fg=color,
    )


if __name__ == "__main__":
    app()
