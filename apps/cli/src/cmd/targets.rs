use anyhow::Result;
use clap::Subcommand;
use comfy_table::{Table, presets::UTF8_BORDERS_ONLY};
use crate::api::Api;
use super::fmt_ts;

#[derive(Subcommand)]
pub enum Cmd {
    /// List all targets with health status
    List {
        /// Filter by health state (e.g. healthy, stale, unhealthy, unknown)
        #[arg(long)]
        health: Option<String>,
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
        println!("No targets{}.", health_filter.map(|s| format!(" with health '{s}'")).unwrap_or_default());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Target", "Health", "Node", "Last seen"]);

    for t in &targets {
        let last_seen = t["last_seen"].as_i64()
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
