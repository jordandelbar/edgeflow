use super::fmt_ts;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};
use edgeflow_client::Api;
use serde_json::Value;

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

pub fn fetch(cmd: &Cmd, api: &Api) -> Result<Value> {
    match cmd {
        Cmd::List { target } => api.list_deployments(target.as_deref()),
        Cmd::Status { target } => api.latest_deployment(target),
    }
}

pub fn render_table(cmd: &Cmd, value: &Value, api: &Api) {
    match cmd {
        Cmd::List { .. } => render_list(value),
        Cmd::Status { target } => render_status(value, target, api),
    }
}

fn render_list(value: &Value) {
    let deps = value["deployments"].as_array().cloned().unwrap_or_default();

    if deps.is_empty() {
        println!("No deployments.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["ID", "Target", "Model", "State", "Created"]);

    for d in &deps {
        let dep_id = d["deployment_id"].as_str().unwrap_or("-");
        let model = match (d["model_name"].as_str(), d["model_version"].as_str()) {
            (Some(n), Some(v)) => format!("{n} v{v}"),
            _ => d["run_id"]
                .as_str()
                .map(|id| id[..12.min(id.len())].to_string())
                .unwrap_or_else(|| "-".into()),
        };

        table.add_row([
            &dep_id[..8.min(dep_id.len())],
            d["target"].as_str().unwrap_or("-"),
            &model,
            d["state"].as_str().unwrap_or("-"),
            &fmt_ts(d["created_at"].as_i64().unwrap_or(0)),
        ]);
    }

    println!("{table}");
}

fn render_status(value: &Value, target: &str, api: &Api) {
    let d = &value["deployment"];

    let model = match (d["model_name"].as_str(), d["model_version"].as_str()) {
        (Some(n), Some(v)) => format!("{n} v{v}"),
        _ => d["run_id"]
            .as_str()
            .map(|id| id[..12.min(id.len())].to_string())
            .unwrap_or_else(|| "-".into()),
    };

    println!("Target:  {target}");
    println!("Model:   {model}");
    println!("State:   {}", d["state"].as_str().unwrap_or("-"));
    println!("ID:      {}", d["deployment_id"].as_str().unwrap_or("-"));
    println!("Created: {}", fmt_ts(d["created_at"].as_i64().unwrap_or(0)));

    // Secondary fetch for the resource specs - presentation enrichment, kept
    // out of fetch() so the JSON contract stays one-API-call-per-command.
    if let Ok(tgt_res) = api.get_target(target) {
        let r = &tgt_res["target"]["resources"];
        if !r.is_null() {
            println!();
            println!("Resources:");
            println!(
                "  CPU request:    {}",
                r["cpu_request"].as_str().unwrap_or("-")
            );
            println!(
                "  Memory request: {}",
                r["memory_request"].as_str().unwrap_or("-")
            );
            println!(
                "  Memory limit:   {}",
                r["memory_limit"].as_str().unwrap_or("-")
            );
            println!(
                "  Sessions:       {}",
                r["sessions"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into())
            );
            println!(
                "  Max concurrent: {}",
                r["max_concurrent"]
                    .as_i64()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".into())
            );
        }
    }
}
