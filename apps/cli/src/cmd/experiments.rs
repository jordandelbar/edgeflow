use super::fmt_ts;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};
use edgeflow_client::Api;
use serde_json::Value;

#[derive(Subcommand)]
pub enum Cmd {
    /// List all experiments
    List,
    /// Show runs for an experiment (accepts name or ID)
    Runs {
        /// Experiment name or ID
        experiment: String,
    },
}

pub fn fetch(cmd: &Cmd, api: &Api) -> Result<Value> {
    match cmd {
        Cmd::List => api.list_experiments(),
        Cmd::Runs { experiment } => {
            let exp = api.resolve_experiment(experiment)?;
            let exp_id = exp["experiment"]["experiment_id"]
                .as_str()
                .unwrap_or(experiment);
            api.search_runs(exp_id)
        }
    }
}

pub fn render_table(cmd: &Cmd, value: &Value, api: &Api) {
    match cmd {
        Cmd::List => render_list(value),
        Cmd::Runs { experiment } => render_runs(value, experiment, api),
    }
}

fn render_list(value: &Value) {
    let exps = value["experiments"].as_array().cloned().unwrap_or_default();
    if exps.is_empty() {
        println!("No experiments.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["ID", "Name", "Created"]);

    for e in &exps {
        table.add_row([
            e["experiment_id"].as_str().unwrap_or("-"),
            e["name"].as_str().unwrap_or("-"),
            &fmt_ts(e["creation_time"].as_i64().unwrap_or(0)),
        ]);
    }

    println!("{table}");
}

fn render_runs(value: &Value, experiment: &str, api: &Api) {
    let runs = value["runs"].as_array().cloned().unwrap_or_default();

    // Re-resolve the experiment for the human-friendly header. JSON consumers
    // didn't see this lookup; it's a presentation detail.
    let display_name = api
        .resolve_experiment(experiment)
        .ok()
        .and_then(|exp| exp["experiment"]["name"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| experiment.to_string());

    if runs.is_empty() {
        println!("No runs in experiment '{display_name}'.");
        return;
    }

    println!("Experiment: {display_name}\n");

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Run ID", "Name", "Status", "Started", "Duration"]);

    for r in &runs {
        let info = &r["info"];
        let run_id = info["run_id"].as_str().unwrap_or("-");
        let run_name = info["run_name"].as_str().unwrap_or("-");
        let status = info["status"].as_str().unwrap_or("-");
        let start = info["start_time"].as_i64().unwrap_or(0);
        let end = info["end_time"].as_i64();
        let duration = end
            .map(|e| {
                let secs = (e - start) / 1000;
                if secs < 60 {
                    format!("{secs}s")
                } else {
                    format!("{}m{}s", secs / 60, secs % 60)
                }
            })
            .unwrap_or_else(|| "running".into());

        table.add_row([run_id, run_name, status, &fmt_ts(start), &duration]);
    }

    println!("{table}");
}
