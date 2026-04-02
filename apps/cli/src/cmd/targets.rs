use super::fmt_ts;
use crate::api::Api;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};

#[derive(Subcommand)]
pub enum Cmd {
    /// List all targets with health status
    List {
        /// Filter by health state (e.g. healthy, stale, unhealthy, unknown)
        #[arg(long)]
        health: Option<String>,
    },
    /// Show full details for a target (specs, resources, loaded model)
    Inspect {
        /// Target name
        target: String,
    },
    /// Update resource settings for a target (merges with existing values)
    SetResources {
        /// Target name
        target: String,
        #[arg(long)]
        cpu_request: Option<String>,
        #[arg(long)]
        memory_request: Option<String>,
        #[arg(long)]
        memory_limit: Option<String>,
        /// Number of ORT sessions (parallel inference workers)
        #[arg(long)]
        sessions: Option<i64>,
        /// Max in-flight requests before 429 (defaults to --sessions)
        #[arg(long)]
        max_concurrent: Option<i64>,
    },
    /// Tear down an inference target (removes pod and deployment record)
    Teardown {
        /// Target name
        target: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List { health } => list(api, health.as_deref()),
        Cmd::Inspect { target } => inspect(api, &target),
        Cmd::SetResources {
            target,
            cpu_request,
            memory_request,
            memory_limit,
            sessions,
            max_concurrent,
        } => set_resources(
            api,
            &target,
            cpu_request.as_deref(),
            memory_request.as_deref(),
            memory_limit.as_deref(),
            sessions,
            max_concurrent,
        ),
        Cmd::Teardown { target, yes } => teardown(api, &target, yes),
    }
}

fn list(api: &Api, health_filter: Option<&str>) -> Result<()> {
    let res = api.list_targets()?;
    let mut targets = res["targets"].as_array().cloned().unwrap_or_default();

    if let Some(state) = health_filter {
        targets.retain(|t| t["health"].as_str() == Some(state));
    }

    if targets.is_empty() {
        println!(
            "No targets{}.",
            health_filter
                .map(|s| format!(" with health '{s}'"))
                .unwrap_or_default()
        );
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Target", "Health", "Node", "Last seen"]);

    for t in &targets {
        let last_seen = t["last_seen"]
            .as_i64()
            .map(|ms| fmt_ts(ms))
            .unwrap_or_else(|| "never".into());

        table.add_row([
            t["target"].as_str().unwrap_or("—"),
            t["health"].as_str().unwrap_or("unknown"),
            t["node"].as_str().unwrap_or("—"),
            &last_seen,
        ]);
    }

    println!("{table}");
    Ok(())
}

fn inspect(api: &Api, target: &str) -> Result<()> {
    let res = api.get_target(target)?;
    let t = &res["target"];

    println!("Target:       {}", t["target"].as_str().unwrap_or("—"));
    println!(
        "Health:       {}",
        t["health"].as_str().unwrap_or("unknown")
    );
    println!("Node:         {}", t["node"].as_str().unwrap_or("—"));
    println!("Pod:          {}", t["pod_name"].as_str().unwrap_or("—"));
    println!("Address:      {}", t["address"].as_str().unwrap_or("—"));

    if let Some(run_id) = t["current_run_id"].as_str() {
        println!("Loaded run:   {run_id}");
    }
    if let Some(loaded_at) = t["model_loaded_at"].as_str() {
        println!("Loaded at:    {loaded_at}");
    }

    let r = &t["resources"];
    println!();
    println!("Resources:");
    println!(
        "  CPU request:    {}",
        r["cpu_request"].as_str().unwrap_or("—")
    );
    println!(
        "  Memory request: {}",
        r["memory_request"].as_str().unwrap_or("—")
    );
    println!(
        "  Memory limit:   {}",
        r["memory_limit"].as_str().unwrap_or("—")
    );
    println!(
        "  Sessions:       {}",
        r["sessions"]
            .as_i64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into())
    );
    println!(
        "  Max concurrent: {}",
        r["max_concurrent"]
            .as_i64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into())
    );

    Ok(())
}

fn set_resources(
    api: &Api,
    target: &str,
    cpu_request: Option<&str>,
    memory_request: Option<&str>,
    memory_limit: Option<&str>,
    sessions: Option<i64>,
    max_concurrent: Option<i64>,
) -> Result<()> {
    let res = api.update_target_resources(
        target,
        cpu_request,
        memory_request,
        memory_limit,
        sessions,
        max_concurrent,
    )?;
    let t = &res["target"];
    let r = &t["resources"];

    println!("Updated resources for '{target}':");
    println!(
        "  CPU request:    {}",
        r["cpu_request"].as_str().unwrap_or("—")
    );
    println!(
        "  Memory request: {}",
        r["memory_request"].as_str().unwrap_or("—")
    );
    println!(
        "  Memory limit:   {}",
        r["memory_limit"].as_str().unwrap_or("—")
    );
    println!(
        "  Sessions:       {}",
        r["sessions"]
            .as_i64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into())
    );
    println!(
        "  Max concurrent: {}",
        r["max_concurrent"]
            .as_i64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into())
    );

    Ok(())
}

fn teardown(api: &Api, target: &str, yes: bool) -> Result<()> {
    if !yes {
        eprint!("Tear down '{target}'? This removes the pod and all deployment records. [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    api.teardown_target(target)?;
    println!("Target '{target}' torn down.");
    Ok(())
}
