use crate::ModelRegistryStore;
use anyhow::Result;
use edgeflow_core::*;
use sqlx::Row;

use super::SqliteStore;

fn row_to_model_version(row: sqlx::sqlite::SqliteRow) -> ModelVersion {
    ModelVersion {
        name: row.get("name"),
        version: row.get::<i64, _>("version").to_string(),
        run_id: row.get("run_id"),
        source: row.get("source"),
        description: row.get("description"),
        current_stage: row.get("current_stage"),
        status: row.get("status"),
        creation_time: row.get("creation_time"),
        last_updated_time: row.get("last_updated_time"),
    }
}

/// Parse `name = 'foo'` and/or `run_id = 'bar'` from a model version filter string.
fn parse_mv_filter(filter: &str) -> (Option<String>, Option<String>) {
    let name = extract_eq_value(filter, "name");
    let run_id = extract_eq_value(filter, "run_id");
    (name, run_id)
}

fn extract_eq_value(filter: &str, field: &str) -> Option<String> {
    let needle = format!("{field} = '");
    let start = filter.find(&needle)? + needle.len();
    let rest = &filter[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

#[async_trait::async_trait]
impl ModelRegistryStore for SqliteStore {
    async fn create_registered_model(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<RegisteredModel> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO registered_models (name, description, creation_time, last_updated_time)
             VALUES (?, ?, ?, ?)",
        )
        .bind(name)
        .bind(description)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                anyhow::anyhow!("already exists: registered model '{name}'")
            } else {
                e.into()
            }
        })?;
        self.get_registered_model(name).await
    }

    async fn get_registered_model(&self, name: &str) -> Result<RegisteredModel> {
        let row = sqlx::query("SELECT name, description, creation_time, last_updated_time FROM registered_models WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool).await?
            .ok_or_else(|| anyhow::anyhow!("not found: registered model '{name}'"))?;

        let mut model = RegisteredModel {
            name: row.get("name"),
            description: row.get("description"),
            creation_time: row.get("creation_time"),
            last_updated_time: row.get("last_updated_time"),
            latest_versions: vec![],
        };
        model.latest_versions = self.list_model_versions(name).await?;
        Ok(model)
    }

    async fn list_registered_models(&self) -> Result<Vec<RegisteredModel>> {
        let rows = sqlx::query("SELECT name FROM registered_models ORDER BY creation_time DESC")
            .fetch_all(&self.pool)
            .await?;
        let mut models = Vec::new();
        for row in rows {
            let name: String = row.get("name");
            models.push(self.get_registered_model(&name).await?);
        }
        Ok(models)
    }

    async fn update_registered_model(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<RegisteredModel> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE registered_models SET description = ?, last_updated_time = ? WHERE name = ?",
        )
        .bind(description)
        .bind(now)
        .bind(name)
        .execute(&self.pool)
        .await?;
        self.get_registered_model(name).await
    }

    async fn delete_registered_model(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM registered_models WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_model_version(
        &self,
        name: &str,
        run_id: Option<&str>,
        source: Option<&str>,
        description: Option<&str>,
    ) -> Result<ModelVersion> {
        let now = chrono::Utc::now().timestamp_millis();
        // Auto-increment version within this model name
        let next_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM model_versions WHERE name = ?",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO model_versions (name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time)
             VALUES (?, ?, ?, ?, ?, 'None', 'READY', ?, ?)"
        )
        .bind(name).bind(next_version).bind(run_id).bind(source)
        .bind(description).bind(now).bind(now)
        .execute(&self.pool).await?;

        sqlx::query("UPDATE registered_models SET last_updated_time = ? WHERE name = ?")
            .bind(now)
            .bind(name)
            .execute(&self.pool)
            .await?;

        self.get_model_version(name, next_version).await
    }

    async fn get_model_version(&self, name: &str, version: i64) -> Result<ModelVersion> {
        let row = sqlx::query(
            "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
             FROM model_versions WHERE name = ? AND version = ?"
        )
        .bind(name).bind(version)
        .fetch_optional(&self.pool).await?
        .ok_or_else(|| anyhow::anyhow!("not found: model version '{name}/{version}'"))?;

        Ok(row_to_model_version(row))
    }

    async fn list_model_versions(&self, name: &str) -> Result<Vec<ModelVersion>> {
        let rows = sqlx::query(
            "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
             FROM model_versions WHERE name = ? ORDER BY version DESC"
        )
        .bind(name).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_model_version).collect())
    }

    async fn get_latest_model_versions(
        &self,
        name: &str,
        stages: &[&str],
    ) -> Result<Vec<ModelVersion>> {
        // If no stages requested, return latest version per each stage that exists.
        // If stages are specified, return the latest version for each requested stage.
        let rows = if stages.is_empty() {
            sqlx::query(
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions
                 WHERE name = ?
                   AND (name, current_stage, version) IN (
                       SELECT name, current_stage, MAX(version) FROM model_versions WHERE name = ? GROUP BY current_stage
                   )
                 ORDER BY version DESC"
            )
            .bind(name).bind(name)
            .fetch_all(&self.pool).await?
        } else {
            // Build a query for the specific stages requested
            let placeholders = stages.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions
                 WHERE name = ?
                   AND current_stage IN ({placeholders})
                   AND (name, current_stage, version) IN (
                       SELECT name, current_stage, MAX(version) FROM model_versions WHERE name = ? GROUP BY current_stage
                   )
                 ORDER BY version DESC"
            );
            let mut q = sqlx::query(&sql).bind(name);
            for s in stages {
                q = q.bind(s);
            }
            q.bind(name).fetch_all(&self.pool).await?
        };
        Ok(rows.into_iter().map(row_to_model_version).collect())
    }

    async fn transition_model_version_stage(
        &self,
        name: &str,
        version: i64,
        stage: &str,
    ) -> Result<ModelVersion> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE model_versions SET current_stage = ?, last_updated_time = ? WHERE name = ? AND version = ?"
        )
        .bind(stage).bind(now).bind(name).bind(version)
        .execute(&self.pool).await?;
        sqlx::query("UPDATE registered_models SET last_updated_time = ? WHERE name = ?")
            .bind(now)
            .bind(name)
            .execute(&self.pool)
            .await?;
        self.get_model_version(name, version).await
    }

    async fn update_model_version(
        &self,
        name: &str,
        version: i64,
        description: Option<&str>,
    ) -> Result<ModelVersion> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE model_versions SET description = ?, last_updated_time = ? WHERE name = ? AND version = ?"
        )
        .bind(description).bind(now).bind(name).bind(version)
        .execute(&self.pool).await?;
        self.get_model_version(name, version).await
    }

    async fn delete_model_version(&self, name: &str, version: i64) -> Result<()> {
        sqlx::query("DELETE FROM model_versions WHERE name = ? AND version = ?")
            .bind(name)
            .bind(version)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn search_model_versions(&self, filter: Option<&str>) -> Result<Vec<ModelVersion>> {
        // Support: name = 'foo' and run_id = 'bar'
        let (name_filter, run_id_filter) = parse_mv_filter(filter.unwrap_or(""));

        let sql = match (&name_filter, &run_id_filter) {
            (Some(_), Some(_)) =>
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions WHERE name = ? AND run_id = ? ORDER BY version DESC".to_string(),
            (Some(_), None) =>
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions WHERE name = ? ORDER BY version DESC".to_string(),
            (None, Some(_)) =>
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions WHERE run_id = ? ORDER BY version DESC".to_string(),
            (None, None) =>
                "SELECT name, version, run_id, source, description, current_stage, status, creation_time, last_updated_time
                 FROM model_versions ORDER BY version DESC".to_string(),
        };

        let mut q = sqlx::query(&sql);
        if let Some(n) = &name_filter {
            q = q.bind(n);
        }
        if let Some(r) = &run_id_filter {
            q = q.bind(r);
        }

        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_model_version).collect())
    }
}
