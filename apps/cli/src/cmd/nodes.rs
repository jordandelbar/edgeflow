use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};
use edgeflow_client::Api;
use serde_json::Value;

#[derive(Subcommand)]
pub enum Cmd {
    /// List all cluster nodes
    List,
}

/// Pure data acquisition - one API call, raw response. JSON consumers and the
/// table renderer both start from this.
pub fn fetch(cmd: &Cmd, api: &Api) -> Result<Value> {
    match cmd {
        Cmd::List => api.list_nodes(),
    }
}

/// Table-mode rendering. Allowed to make extra API calls for join/enrichment;
/// those joins are a presentation concern and shouldn't leak into the raw JSON
/// contract that parity tests pin against.
pub fn render_table(cmd: &Cmd, value: &Value, api: &Api) {
    match cmd {
        Cmd::List => render_list(value, api),
    }
}

fn render_list(value: &Value, api: &Api) {
    let nodes = value["nodes"].as_array().cloned().unwrap_or_default();
    if nodes.is_empty() {
        println!("No nodes found.");
        return;
    }

    let targets_res = api
        .list_targets()
        .unwrap_or_else(|_| serde_json::json!({ "targets": [] }));
    let targets = targets_res["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Node", "Targets", "Unhealthy"]);

    for n in &nodes {
        let name = n.as_str().unwrap_or("?");
        let node_targets: Vec<_> = targets
            .iter()
            .filter(|t| t["node"].as_str() == Some(name))
            .collect();
        let unhealthy = node_targets
            .iter()
            .filter(|t| matches!(t["health"].as_str(), Some("unhealthy") | Some("stale")))
            .count();
        let unhealthy_str = if unhealthy > 0 {
            format!("{unhealthy} ⚠")
        } else {
            "-".into()
        };

        table.add_row([name, &node_targets.len().to_string(), &unhealthy_str]);
    }

    println!("{table}");
}
