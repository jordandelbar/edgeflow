"""edgeflow CLI — deploy runs to inference targets and watch state transitions."""

import typer
import requests

from edgeflow.deploy import deploy as _deploy, _DEFAULT_SERVER

app = typer.Typer(help="edgeflow CLI")


@app.command()
def deploy(
    run_id: str = typer.Argument(..., help="MLflow run ID to deploy"),
    target: str = typer.Argument(..., help="Inference target name (e.g. iris-inference)"),
    server: str = typer.Option(_DEFAULT_SERVER, envvar="EDGEFLOW_SERVER", help="edgeflow server URL"),
    timeout: int = typer.Option(300, help="Seconds to wait for the deployment to become deployed"),
    no_wait: bool = typer.Option(False, "--no-wait", help="Return immediately without polling"),
):
    """Deploy a run to an inference target and wait for it to become deployed."""
    typer.echo(f"Deploying run {run_id} → target '{target}'")
    try:
        deployment = _deploy(run_id, target, server=server, wait=not no_wait, timeout=timeout)
        state = deployment["state"]
        color = typer.colors.GREEN if state == "deployed" else typer.colors.YELLOW
        typer.secho(
            f"deployment_id={deployment['deployment_id']}  state={state}",
            fg=color,
        )
    except RuntimeError as e:
        typer.secho(str(e), fg=typer.colors.RED)
        raise typer.Exit(1)
    except requests.HTTPError as e:
        typer.secho(f"server error: {e}", fg=typer.colors.RED)
        raise typer.Exit(1)


@app.command()
def status(
    target: str = typer.Argument(..., help="Inference target name"),
    server: str = typer.Option(_DEFAULT_SERVER, envvar="EDGEFLOW_SERVER", help="edgeflow server URL"),
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
    color = typer.colors.GREEN if state == "deployed" else (
        typer.colors.RED if state == "failed" else typer.colors.YELLOW
    )
    typer.secho(
        f"target={target}  deployment_id={d['deployment_id']}  run_id={d['run_id']}  state={state}",
        fg=color,
    )


if __name__ == "__main__":
    app()
