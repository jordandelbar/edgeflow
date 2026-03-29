use anyhow::Result;
use clap::Subcommand;
use crate::api::Api;
use super::fmt_ts;

#[derive(Subcommand)]
pub enum Cmd {
    /// Show details for a run (params, metrics, tags)
    Get {
        /// Run ID
        run_id: String,
    },
}

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::Get { run_id } => get(api, &run_id),
    }
}

fn get(api: &Api, run_id: &str) -> Result<()> {
    let run_id = api.resolve_run_id(run_id)?;
    let res = api.get_run(&run_id)?;
    let r = &res["run"];
    let info = &r["info"];
    let data = &r["data"];

    let name = info["run_name"].as_str().unwrap_or("—");
    let status = info["status"].as_str().unwrap_or("—");
    let start = info["start_time"].as_i64().unwrap_or(0);
    let end = info["end_time"].as_i64();

    println!("Run:     {}", &run_id[..12.min(run_id.len())]);
    println!("Name:    {name}");
    println!("Status:  {status}");
    println!("Started: {}", fmt_ts(start));
    if let Some(e) = end {
        let secs = (e - start) / 1000;
        println!("Duration: {}s", secs);
    }

    if let Some(params) = data["params"].as_array() {
        if !params.is_empty() {
            println!("\nParams:");
            for p in params {
                println!("  {} = {}", p["key"].as_str().unwrap_or("?"), p["value"].as_str().unwrap_or("?"));
            }
        }
    }

    if let Some(metrics) = data["metrics"].as_array() {
        if !metrics.is_empty() {
            println!("\nMetrics:");
            for m in metrics {
                println!("  {} = {}", m["key"].as_str().unwrap_or("?"), m["value"]);
            }
        }
    }

    if let Some(tags) = data["tags"].as_array() {
        let visible: Vec<_> = tags.iter()
            .filter(|t| !t["key"].as_str().unwrap_or("").starts_with("mlflow."))
            .collect();
        if !visible.is_empty() {
            println!("\nTags:");
            for t in visible {
                println!("  {} = {}", t["key"].as_str().unwrap_or("?"), t["value"].as_str().unwrap_or("?"));
            }
        }
    }

    Ok(())
}
