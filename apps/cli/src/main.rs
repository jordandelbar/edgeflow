mod api;
mod cmd;

use anyhow::Result;
use api::Api;
use clap::{Parser, Subcommand};

const DEFAULT_SERVER: &str = "http://localhost:5000";

#[derive(Parser)]
#[command(
    name = "edgeflow",
    about = "Manage edgeflow experiments, models and deployments",
    version
)]
struct Cli {
    /// edgeflow server URL
    #[arg(long, env = "EDGEFLOW_SERVER", global = true, default_value = DEFAULT_SERVER)]
    server: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage experiments
    #[command(subcommand)]
    Experiments(cmd::experiments::Cmd),

    /// Manage runs
    #[command(subcommand)]
    Runs(cmd::runs::Cmd),

    /// Manage registered models and versions
    #[command(subcommand)]
    Models(cmd::models::Cmd),

    /// Deploy a registered model version to a target
    Deploy {
        /// Registered model name
        model_name: String,
        /// Version number
        model_version: String,
        /// Inference target name
        target: String,
        /// Number of ORT sessions (parallel inference workers)
        #[arg(long)]
        sessions: Option<i64>,
        /// Max in-flight requests before 429 (defaults to --sessions)
        #[arg(long)]
        max_concurrent: Option<i64>,
        /// Poll until the deployment reaches a terminal state
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        wait: bool,
        /// Polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },

    /// Manage deployments
    #[command(subcommand)]
    Deployments(cmd::deployments::Cmd),

    /// Manage inference targets
    #[command(subcommand)]
    Targets(cmd::targets::Cmd),

    /// List cluster nodes
    #[command(subcommand)]
    Nodes(cmd::nodes::Cmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = Api::new(&cli.server);

    match cli.command {
        Command::Experiments(cmd) => cmd::experiments::run(cmd, &api),
        Command::Runs(cmd) => cmd::runs::run(cmd, &api),
        Command::Models(cmd) => cmd::models::run(cmd, &api),
        Command::Deployments(cmd) => cmd::deployments::run(cmd, &api),
        Command::Targets(cmd) => cmd::targets::run(cmd, &api),
        Command::Nodes(cmd) => cmd::nodes::run(cmd, &api),
        Command::Deploy {
            model_name,
            model_version,
            target,
            sessions,
            max_concurrent,
            wait,
            timeout,
        } => deploy(
            &api,
            &model_name,
            &model_version,
            &target,
            sessions,
            max_concurrent,
            wait,
            timeout,
        ),
    }
}

fn deploy(
    api: &Api,
    model_name: &str,
    model_version: &str,
    target: &str,
    sessions: Option<i64>,
    max_concurrent: Option<i64>,
    wait: bool,
    timeout: u64,
) -> Result<()> {
    println!("Deploying {model_name} v{model_version} → '{target}'");

    let res = api.create_deployment(model_name, model_version, target, sessions, max_concurrent)?;
    let dep = &res["deployment"];
    let dep_id = dep["deployment_id"].as_str().unwrap_or("?");
    println!("deployment_id: {dep_id}");

    if !wait {
        return Ok(());
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut last_state = dep["state"].as_str().unwrap_or("pending").to_string();

    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("timed out after {timeout}s — last state: {last_state}");
        }
        std::thread::sleep(std::time::Duration::from_secs(2));

        let res = api.get_deployment(dep_id)?;
        let state = res["deployment"]["state"]
            .as_str()
            .unwrap_or("?")
            .to_string();

        if state != last_state {
            println!("{last_state} → {state}");
            last_state = state.clone();
        }

        match state.as_str() {
            "deployed" => {
                println!("Deployment live on '{target}'.");
                return Ok(());
            }
            "failed" => anyhow::bail!("deployment failed"),
            "superseded" => anyhow::bail!("deployment was superseded"),
            _ => {}
        }
    }
}
