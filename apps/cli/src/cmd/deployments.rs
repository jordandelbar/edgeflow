use super::fmt_ts;
use crate::api::Api;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};

#[derive(Subcommand)]
pub enum Cmd {
    /// List deployments, optionally filtered by target
    List {
        /// Filter by target name
        #[arg(long)]
        target: Option<String>,
    },
    /// Show latest deployment state for a target
    Status {
        /// Target name
        target: String,
    },
}

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List { target } => list(api, target.as_deref()),
        Cmd::Status { target } => status(api, &target),
    }
}

fn list(api: &Api, target: Option<&str>) -> Result<()> {
    let res = api.list_deployments(target)?;
    let deps = res["deployments"].as_array().cloned().unwrap_or_default();

    if deps.is_empty() {
        println!("No deployments.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["ID", "Target", "Model", "State", "Created"]);

    for d in &deps {
        let dep_id = d["deployment_id"].as_str().unwrap_or("—");
        let model = match (d["model_name"].as_str(), d["model_version"].as_str()) {
            (Some(n), Some(v)) => format!("{n} v{v}"),
            _ => d["run_id"]
                .as_str()
                .map(|id| id[..12.min(id.len())].to_string())
                .unwrap_or_else(|| "—".into()),
        };

        table.add_row([
            &dep_id[..8.min(dep_id.len())],
            d["target"].as_str().unwrap_or("—"),
            &model,
            d["state"].as_str().unwrap_or("—"),
            &fmt_ts(d["created_at"].as_i64().unwrap_or(0)),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn status(api: &Api, target: &str) -> Result<()> {
    let res = api.latest_deployment(target)?;
    let d = &res["deployment"];

    let model = match (d["model_name"].as_str(), d["model_version"].as_str()) {
        (Some(n), Some(v)) => format!("{n} v{v}"),
        _ => d["run_id"]
            .as_str()
            .map(|id| id[..12.min(id.len())].to_string())
            .unwrap_or_else(|| "—".into()),
    };

    let state = d["state"].as_str().unwrap_or("—");

    println!("Target:  {target}");
    println!("Model:   {model}");
    println!("State:   {state}");
    println!("Created: {}", fmt_ts(d["created_at"].as_i64().unwrap_or(0)));

    Ok(())
}
