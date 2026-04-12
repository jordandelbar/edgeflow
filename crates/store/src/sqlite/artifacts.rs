use crate::ArtifactStore;
use anyhow::Result;
use edgeflow_core::*;
use sqlx::Row;
use std::path::PathBuf;

use super::SqliteStore;

#[async_trait::async_trait]
impl ArtifactStore for SqliteStore {
    async fn list_artifacts(&self, run_id: &str, path: Option<&str>) -> Result<Vec<FileInfo>> {
        let root = self.artifact_root(run_id).await?;
        let dir = match path {
            Some(p) => root.join(p),
            None => root,
        };

        let mut files = Vec::new();
        if dir.exists() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                let relative = entry
                    .path()
                    .strip_prefix(&self.artifact_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| entry.path().display().to_string());
                files.push(FileInfo {
                    path: relative,
                    is_dir: meta.is_dir(),
                    file_size: if meta.is_file() {
                        Some(meta.len() as i64)
                    } else {
                        None
                    },
                });
            }
        }
        Ok(files)
    }

    async fn artifact_root(&self, run_id: &str) -> Result<PathBuf> {
        let row = sqlx::query("SELECT artifact_uri FROM runs WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&self.pool)
            .await?;
        let uri: String = row.get("artifact_uri");
        let rel = uri.strip_prefix("mlflow-artifacts:/").unwrap_or(&uri);
        Ok(self.artifact_root.join(rel))
    }
}
