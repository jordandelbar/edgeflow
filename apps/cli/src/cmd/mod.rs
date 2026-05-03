pub mod deploy;
pub mod deployments;
pub mod experiments;
pub mod models;
pub mod nodes;
pub mod runs;
pub mod targets;

use anyhow::Result;
use chrono::DateTime;
use serde_json::Value;

/// Output format selected by the user. Set once in `main.rs` from the global
/// `--json` flag and dispatched per command.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
}

impl Format {
    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Pretty-print a `Value` as JSON. The shared JSON renderer for every command:
/// since the underlying client already returns `serde_json::Value`, JSON output
/// is uniform and lives here rather than per-module.
pub fn emit_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Format a millisecond timestamp as a human-readable local date/time.
pub fn fmt_ts(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%d %b %Y %H:%M").to_string())
        .unwrap_or_else(|| "-".into())
}

/// Truncate a string to at most `n` characters.
pub fn trunc(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n - 1])
    }
}
