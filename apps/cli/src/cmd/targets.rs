use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};
use edgeflow_client::Api;
use serde_json::Value;

use super::Format;

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
        /// Number of ORT sessions (parallel inference workers)
        #[arg(long)]
        sessions: Option<i64>,
        /// Max in-flight requests before 429 (defaults to --sessions)
        #[arg(long)]
        max_concurrent: Option<i64>,
        // ── k8s infrastructure ──────────────────────────────────────────
        #[arg(long)]
        cpu_request: Option<String>,
        #[arg(long)]
        memory_request: Option<String>,
        #[arg(long)]
        memory_limit: Option<String>,
        /// Number of pod replicas
        #[arg(long)]
        replicas: Option<i64>,
        /// Pod placement strategy: spread (anti-affinity), pack (affinity), or none
        #[arg(long, value_name = "STRATEGY")]
        placement: Option<String>,
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

/// Pre-fetch interaction. Returns `Ok(false)` if the user aborts and the
/// command should exit cleanly without calling the API. Json mode is treated
/// as non-interactive: machines won't answer y/N, so we require `--yes`
/// explicitly for destructive ops in that mode.
pub fn confirm(cmd: &Cmd, format: Format) -> Result<bool> {
    let Cmd::Teardown { target, yes } = cmd else {
        return Ok(true);
    };
    if *yes {
        return Ok(true);
    }
    if format.is_json() {
        anyhow::bail!("teardown requires --yes when --json is set (non-interactive)");
    }
    eprint!("Tear down '{target}'? This removes the pod and all deployment records. [y/N] ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") {
        Ok(true)
    } else {
        println!("Aborted.");
        Ok(false)
    }
}

pub fn fetch(cmd: &Cmd, api: &Api) -> Result<Value> {
    match cmd {
        Cmd::List { .. } => api.list_targets(),
        Cmd::Inspect { target } => api.get_target(target),
        Cmd::SetResources {
            target,
            sessions,
            max_concurrent,
            cpu_request,
            memory_request,
            memory_limit,
            replicas,
            placement,
        } => api.update_target_resources(
            target,
            *sessions,
            *max_concurrent,
            cpu_request.as_deref(),
            memory_request.as_deref(),
            memory_limit.as_deref(),
            *replicas,
            placement.as_deref(),
        ),
        Cmd::Teardown { target, .. } => {
            api.teardown_target(target)?;
            // Synthetic ack so the JSON renderer has something to emit and the
            // shape stays uniform across mutating commands.
            Ok(serde_json::json!({"target": target, "status": "torn_down"}))
        }
    }
}

pub fn render_table(cmd: &Cmd, value: &Value) {
    match cmd {
        Cmd::List { health } => render_list(value, health.as_deref()),
        Cmd::Inspect { .. } => render_inspect(value),
        Cmd::SetResources { target, .. } => render_set_resources(value, target),
        Cmd::Teardown { target, .. } => println!("Target '{target}' torn down."),
    }
}

fn render_list(value: &Value, health_filter: Option<&str>) {
    let mut targets = value["targets"].as_array().cloned().unwrap_or_default();

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
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Target", "Health", "Node"]);

    for t in &targets {
        table.add_row([
            t["target"].as_str().unwrap_or("-"),
            t["health"].as_str().unwrap_or("unknown"),
            t["node"].as_str().unwrap_or("-"),
        ]);
    }

    println!("{table}");
}

fn render_inspect(value: &Value) {
    let t = &value["target"];

    println!("Target:       {}", t["target"].as_str().unwrap_or("-"));
    println!(
        "Health:       {}",
        t["health"].as_str().unwrap_or("unknown")
    );
    println!("Node:         {}", t["node"].as_str().unwrap_or("-"));

    if let Some(pods) = t["pods"].as_array() {
        if pods.is_empty() {
            println!("Pods:         -");
        } else {
            for (i, pod) in pods.iter().enumerate() {
                let prefix = if i == 0 { "Pods:" } else { "     " };
                println!(
                    "{:<14}{} ({}) - {}",
                    prefix,
                    pod["pod_id"].as_str().unwrap_or("?"),
                    pod["address"].as_str().unwrap_or("?"),
                    pod["health"].as_str().unwrap_or("unknown"),
                );
            }
        }
    }

    if let Some(run_id) = t["current_run_id"].as_str() {
        println!("Loaded run:   {run_id}");
    }
    if let Some(loaded_at) = t["model_loaded_at"].as_str() {
        println!("Loaded at:    {loaded_at}");
    }

    let r = &t["resources"];
    println!();
    println!("Resources (edgeflow):");
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

    let inf = &t["infra"];
    if !inf.is_null() {
        println!();
        println!("Infrastructure (k8s):");
        println!(
            "  CPU request:    {}",
            inf["cpu_request"].as_str().unwrap_or("-")
        );
        println!(
            "  Memory request: {}",
            inf["memory_request"].as_str().unwrap_or("-")
        );
        println!(
            "  Memory limit:   {}",
            inf["memory_limit"].as_str().unwrap_or("-")
        );
        println!(
            "  Replicas:       {}",
            inf["replicas"]
                .as_i64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into())
        );
        if let Some(p) = inf["placement"].as_str() {
            let desc = match p {
                "spread" => "spread (anti-affinity, different nodes)",
                "pack" => "pack (affinity, same node)",
                other => other,
            };
            println!("  Placement:      {desc}");
        }
        if let Some(ns) = inf["node_selector"].as_object() {
            let pairs: Vec<String> = ns
                .iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("?")))
                .collect();
            println!("  Node selector:  {}", pairs.join(", "));
        }
    }
}

fn render_set_resources(value: &Value, target: &str) {
    let t = &value["target"];
    let r = &t["resources"];
    let inf = &t["infra"];

    println!("Updated resources for '{target}':");
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
    if !inf.is_null() {
        println!("Infrastructure (from k8s):");
        println!(
            "  CPU request:    {}",
            inf["cpu_request"].as_str().unwrap_or("-")
        );
        println!(
            "  Memory request: {}",
            inf["memory_request"].as_str().unwrap_or("-")
        );
        println!(
            "  Memory limit:   {}",
            inf["memory_limit"].as_str().unwrap_or("-")
        );
        println!(
            "  Replicas:       {}",
            inf["replicas"]
                .as_i64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
}
