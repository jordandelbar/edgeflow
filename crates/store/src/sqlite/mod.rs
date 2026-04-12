mod artifacts;
mod deployments;
mod experiments;
mod metrics;
mod models;
mod runs;
mod targets;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

/// Parse `tag.\`key\` = 'value'` or `tag.'key' = 'value'` into `(key, value)`.
pub(crate) fn parse_tag_filter(filter: &str) -> Option<(String, String)> {
    let s = filter.trim();
    let s = s.strip_prefix("tag.")?;
    let (key, rest) = if let Some(inner) = s.strip_prefix('`') {
        let end = inner.find('`')?;
        (inner[..end].to_string(), inner[end + 1..].trim())
    } else if let Some(inner) = s.strip_prefix('\'') {
        let end = inner.find('\'')?;
        (inner[..end].to_string(), inner[end + 1..].trim())
    } else {
        return None;
    };
    let rest = rest.strip_prefix('=')?.trim();
    let value = rest.strip_prefix('\'')?.strip_suffix('\'')?;
    Some((key, value.to_string()))
}

pub struct SqliteStore {
    pub(crate) pool: SqlitePool,
    pub(crate) artifact_root: PathBuf,
}

impl SqliteStore {
    pub async fn new(db_path: &Path, artifact_root: PathBuf) -> Result<Self> {
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&url)
            .await
            .context("failed to open sqlite database")?;

        sqlx::migrate!()
            .run(&pool)
            .await
            .context("failed to run migrations")?;

        Ok(Self {
            pool,
            artifact_root,
        })
    }
}
