pub mod deployments;
pub mod experiments;
pub mod models;
pub mod nodes;
pub mod runs;
pub mod targets;

use chrono::DateTime;

/// Format a millisecond timestamp as a human-readable local date/time.
pub fn fmt_ts(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%d %b %Y %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

/// Truncate a string to at most `n` characters.
pub fn trunc(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{}…", &s[..n - 1]) }
}
