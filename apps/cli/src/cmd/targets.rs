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

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List { health } => list(api, health.as_deref()),
        Cmd::Inspect { target } => inspect(api, &target),
        Cmd::SetResources {
            target,
            sessions,
            max_concurrent,
            cpu_request,
            memory_request,
            memory_limit,
            replicas,
            placement,
        } => set_resources(
            api,
            &target,
            sessions,
            max_concurrent,
            cpu_request.as_deref(),
            memory_request.as_deref(),
            memory_limit.as_deref(),
            replicas,
            placement.as_deref(),
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
    table.set_header(["Target", "Health", "Node"]);

    for t in &targets {
        table.add_row([
            t["target"].as_str().unwrap_or("-"),
            t["health"].as_str().unwrap_or("unknown"),
            t["node"].as_str().unwrap_or("-"),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn inspect(api: &Api, target: &str) -> Result<()> {
    let res = api.get_target(target)?;
    let t = &res["target"];

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

    Ok(())
}

fn set_resources(
    api: &Api,
    target: &str,
    sessions: Option<i64>,
    max_concurrent: Option<i64>,
    cpu_request: Option<&str>,
    memory_request: Option<&str>,
    memory_limit: Option<&str>,
    replicas: Option<i64>,
    placement: Option<&str>,
) -> Result<()> {
    let res = api.update_target_resources(
        target,
        sessions,
        max_concurrent,
        cpu_request,
        memory_request,
        memory_limit,
        replicas,
        placement,
    )?;
    let t = &res["target"];
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
