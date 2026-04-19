use super::{fmt_ts, trunc};
use crate::api::Api;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_BORDERS_ONLY, Table};

#[derive(Subcommand)]
pub enum Cmd {
    /// List all registered models
    List,
    /// Show all versions of a model
    Versions {
        /// Registered model name
        name: String,
    },
    /// Register a run as a new model version
    Register {
        /// MLflow run ID
        run_id: String,
        /// Registered model name
        name: String,
    },
    /// Transition a model version to a new stage
    Stage {
        /// Registered model name
        name: String,
        /// Version number
        version: String,
        /// New stage (e.g. Production, Staging, Archived, None)
        stage: String,
    },
    /// Delete a registered model and all its versions
    Delete {
        /// Registered model name
        name: String,
    },
    /// Delete a specific model version
    DeleteVersion {
        /// Registered model name
        name: String,
        /// Version number
        version: String,
    },
}

pub fn run(cmd: Cmd, api: &Api) -> Result<()> {
    match cmd {
        Cmd::List => list(api),
        Cmd::Versions { name } => versions(api, &name),
        Cmd::Register { run_id, name } => register(api, &run_id, &name),
        Cmd::Stage {
            name,
            version,
            stage,
        } => stage_cmd(api, &name, &version, &stage),
        Cmd::Delete { name } => delete(api, &name),
        Cmd::DeleteVersion { name, version } => delete_version(api, &name, &version),
    }
}

fn list(api: &Api) -> Result<()> {
    let res = api.list_registered_models()?;
    let models = res["registered_models"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if models.is_empty() {
        println!("No registered models.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Name", "Versions", "Latest", "Stage", "Updated"]);

    for m in &models {
        let name = m["name"].as_str().unwrap_or("-");
        let versions = m["latest_versions"].as_array().cloned().unwrap_or_default();
        let count = versions.len();
        let latest = versions.iter().max_by_key(|v| {
            v["version"]
                .as_str()
                .unwrap_or("0")
                .parse::<i64>()
                .unwrap_or(0)
        });
        let (latest_v, stage) = latest
            .map(|v| {
                (
                    format!("v{}", v["version"].as_str().unwrap_or("?")),
                    v["current_stage"].as_str().unwrap_or("None").to_string(),
                )
            })
            .unwrap_or_else(|| ("-".into(), "-".into()));

        table.add_row([
            name,
            &count.to_string(),
            &latest_v,
            &stage,
            &fmt_ts(m["last_updated_time"].as_i64().unwrap_or(0)),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn versions(api: &Api, name: &str) -> Result<()> {
    let res = api.list_model_versions(name)?;
    let versions = res["model_versions"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    if versions.is_empty() {
        println!("No versions for model '{name}'.");
        return Ok(());
    }

    println!("Model: {name}\n");

    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(["Version", "Stage", "Run ID", "Status", "Created"]);

    for v in &versions {
        let run_id = v["run_id"].as_str().unwrap_or("-");
        table.add_row([
            &format!("v{}", v["version"].as_str().unwrap_or("?")),
            v["current_stage"].as_str().unwrap_or("None"),
            &trunc(run_id, 12),
            v["status"].as_str().unwrap_or("-"),
            &fmt_ts(v["creation_time"].as_i64().unwrap_or(0)),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn register(api: &Api, run_id: &str, name: &str) -> Result<()> {
    let res = api.register_model(run_id, name)?;
    let mv = &res["model_version"];
    println!(
        "Registered: {} v{}",
        mv["name"].as_str().unwrap_or(name),
        mv["version"].as_str().unwrap_or("?"),
    );
    Ok(())
}

fn stage_cmd(api: &Api, name: &str, version: &str, stage: &str) -> Result<()> {
    let res = api.transition_stage(name, version, stage)?;
    let mv = &res["model_version"];
    println!(
        "{} v{} → {}",
        mv["name"].as_str().unwrap_or(name),
        mv["version"].as_str().unwrap_or(version),
        mv["current_stage"].as_str().unwrap_or(stage),
    );
    Ok(())
}

fn delete(api: &Api, name: &str) -> Result<()> {
    api.delete_registered_model(name)?;
    println!("Deleted model '{name}'.");
    Ok(())
}

fn delete_version(api: &Api, name: &str, version: &str) -> Result<()> {
    api.delete_model_version(name, version)?;
    println!("Deleted {name} v{version}.");
    Ok(())
}
