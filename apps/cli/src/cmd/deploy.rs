//! `deploy` doesn't fit the one-API-call shape of the other commands - it
//! creates a deployment, then polls for terminal state. Producer emits a
//! stream of `Event`s; renderers consume them. Same fetch/render split, just
//! with an iterator-of-events instead of a single `Value`.

use anyhow::Result;
use edgeflow_client::Api;
use serde_json::Value;

pub struct Args<'a> {
    pub model_name: &'a str,
    pub model_version: &'a str,
    pub target: &'a str,
    pub sessions: Option<i64>,
    pub max_concurrent: Option<i64>,
    pub wait: bool,
    pub timeout: u64,
}

pub enum Event {
    /// Initial response from `create_deployment`.
    Created(Value),
    /// Polled state changed (only emitted when the new state differs).
    StateChanged { from: String, to: String },
    /// Terminal state reached (deployed). Carries the final `get_deployment`
    /// response - this is what the JSON renderer emits.
    Final(Value),
}

pub fn run(args: &Args, api: &Api, mut on_event: impl FnMut(&Event) -> Result<()>) -> Result<()> {
    let res = api.create_deployment(
        args.model_name,
        args.model_version,
        args.target,
        args.sessions,
        args.max_concurrent,
    )?;
    let dep_id = res["deployment"]["deployment_id"]
        .as_str()
        .unwrap_or("?")
        .to_string();
    let initial_state = res["deployment"]["state"]
        .as_str()
        .unwrap_or("pending")
        .to_string();
    on_event(&Event::Created(res.clone()))?;

    if !args.wait {
        on_event(&Event::Final(res))?;
        return Ok(());
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.timeout);
    let mut last_state = initial_state;

    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s - last state: {last_state}",
                args.timeout
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(2));

        let res = api.get_deployment(&dep_id)?;
        let state = res["deployment"]["state"]
            .as_str()
            .unwrap_or("?")
            .to_string();

        if state != last_state {
            on_event(&Event::StateChanged {
                from: last_state.clone(),
                to: state.clone(),
            })?;
            last_state = state.clone();
        }

        match state.as_str() {
            "deployed" => {
                on_event(&Event::Final(res))?;
                return Ok(());
            }
            "failed" => anyhow::bail!("deployment failed"),
            "superseded" => anyhow::bail!("deployment was superseded"),
            _ => {}
        }
    }
}

/// Table-mode event handler. Prints the header up front, then transitions and
/// the final live-on confirmation as events arrive.
pub fn render_table(args: &Args, api: &Api) -> Result<()> {
    println!(
        "Deploying {} v{} → '{}'",
        args.model_name, args.model_version, args.target
    );
    let target = args.target;
    run(args, api, |ev| {
        match ev {
            Event::Created(v) => {
                let id = v["deployment"]["deployment_id"].as_str().unwrap_or("?");
                println!("deployment_id: {id}");
            }
            Event::StateChanged { from, to } => println!("{from} → {to}"),
            Event::Final(_) => println!("Deployment live on '{target}'."),
        }
        Ok(())
    })
}

/// JSON-mode event handler. Drains the stream silently and emits the final
/// deployment response so consumers get one well-formed JSON document.
pub fn render_json(args: &Args, api: &Api) -> Result<()> {
    let mut final_value: Option<Value> = None;
    run(args, api, |ev| {
        if let Event::Final(v) = ev {
            final_value = Some(v.clone());
        }
        Ok(())
    })?;
    super::emit_json(&final_value.unwrap_or(Value::Null))
}
