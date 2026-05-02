use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};
use edgeflow_client::Api;

#[derive(Subcommand)]
pub enum Cmd {
    /// List all cluster nodes
    List,
}

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List => list(api),
    }
}

fn list(api: &Api) -> Result<()> {
    let nodes_res = api.list_nodes()?;
    let nodes = nodes_res["nodes"].as_array().cloned().unwrap_or_default();

    if nodes.is_empty() {
        println!("No nodes found.");
        return Ok(());
    }

    // Enrich with target counts per node.
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
    Ok(())
}
