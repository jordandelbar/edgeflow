mod cmd;

use anyhow::Result;
use clap::{Parser, Subcommand};
use edgeflow_client::Api;

use cmd::Format;

#[derive(Parser)]
#[command(
    name = "edgeflow",
    about = "Manage edgeflow experiments, models and deployments",
    version
)]
struct Cli {
    /// edgeflow server URL (required: pass --server or set EDGEFLOW_SERVER).
    /// No localhost fallback - silent defaults mask misconfiguration.
    #[arg(long, env = "EDGEFLOW_SERVER", global = true)]
    server: Option<String>,

    /// Emit raw JSON from the client instead of formatted tables.
    #[arg(long, global = true)]
    json: bool,

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
    let server = cli.server.ok_or_else(|| {
        anyhow::anyhow!("no edgeflow server configured: pass --server <URL> or set EDGEFLOW_SERVER")
    })?;
    let api = Api::new(&server);
    let format = if cli.json {
        Format::Json
    } else {
        Format::Table
    };

    match cli.command {
        Command::Experiments(c) => {
            let v = cmd::experiments::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::experiments::render_table(&c, &v, &api),
            }
        }
        Command::Runs(c) => {
            let v = cmd::runs::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::runs::render_table(&c, &v),
            }
        }
        Command::Models(c) => {
            let v = cmd::models::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::models::render_table(&c, &v),
            }
        }
        Command::Deployments(c) => {
            let v = cmd::deployments::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::deployments::render_table(&c, &v, &api),
            }
        }
        Command::Targets(c) => {
            if !cmd::targets::confirm(&c, format)? {
                return Ok(());
            }
            let v = cmd::targets::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::targets::render_table(&c, &v),
            }
        }
        Command::Nodes(c) => {
            let v = cmd::nodes::fetch(&c, &api)?;
            match format {
                Format::Json => cmd::emit_json(&v)?,
                Format::Table => cmd::nodes::render_table(&c, &v, &api),
            }
        }
        Command::Deploy {
            model_name,
            model_version,
            target,
            sessions,
            max_concurrent,
            wait,
            timeout,
        } => {
            let args = cmd::deploy::Args {
                model_name: &model_name,
                model_version: &model_version,
                target: &target,
                sessions,
                max_concurrent,
                wait,
                timeout,
            };
            match format {
                Format::Json => cmd::deploy::render_json(&args, &api)?,
                Format::Table => cmd::deploy::render_table(&args, &api)?,
            }
        }
    }
    Ok(())
}
