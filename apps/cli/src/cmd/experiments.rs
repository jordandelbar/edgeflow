use super::fmt_ts;
use crate::api::Api;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};

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

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List => list(api),
        Cmd::Runs { experiment } => runs(api, &experiment),
    }
}

fn list(api: &Api) -> Result<()> {
    let res = api.list_experiments()?;
    let exps = res["experiments"].as_array().cloned().unwrap_or_default();

    if exps.is_empty() {
        println!("No experiments.");
        return Ok(());
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
    Ok(())
}

fn runs(api: &Api, experiment: &str) -> Result<()> {
    let exp = api.resolve_experiment(experiment)?;
    let exp_id = exp["experiment"]["experiment_id"]
        .as_str()
        .unwrap_or(experiment);
    let name = exp["experiment"]["name"].as_str().unwrap_or(experiment);

    let res = api.search_runs(exp_id)?;
    let runs = res["runs"].as_array().cloned().unwrap_or_default();

    if runs.is_empty() {
        println!("No runs in experiment '{name}'.");
        return Ok(());
    }

    println!("Experiment: {name}\n");

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
    Ok(())
}
