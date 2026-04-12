use crate::ExperimentStore;
use anyhow::{Context, Result};
use edgeflow_core::*;
use sqlx::Row;

use super::SqliteStore;

#[async_trait::async_trait]
impl ExperimentStore for SqliteStore {
    async fn create_experiment(
        &self,
        name: &str,
        artifact_location: Option<&str>,
        tags: Vec<ExperimentTag>,
    ) -> Result<Experiment> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let location = artifact_location
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("mlflow-artifacts:/{}", id));

        sqlx::query(
            "INSERT INTO experiments (experiment_id, name, artifact_location, lifecycle_stage, creation_time, last_update_time)
             VALUES (?, ?, ?, 'active', ?, ?)"
        )
        .bind(&id).bind(name).bind(&location).bind(now).bind(now)
        .execute(&self.pool).await?;

        for tag in &tags {
            sqlx::query("INSERT INTO experiment_tags (experiment_id, key, value) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(&tag.key)
                .bind(&tag.value)
                .execute(&self.pool)
                .await?;
        }

        self.get_experiment(&id).await
    }

    async fn get_experiment(&self, experiment_id: &str) -> Result<Experiment> {
        let row = sqlx::query(
            "SELECT experiment_id, name, artifact_location, lifecycle_stage, creation_time, last_update_time
             FROM experiments WHERE experiment_id = ?"
        )
        .bind(experiment_id)
        .fetch_one(&self.pool)
        .await
        .context("experiment not found")?;

        let tag_rows =
            sqlx::query("SELECT key, value FROM experiment_tags WHERE experiment_id = ?")
                .bind(experiment_id)
                .fetch_all(&self.pool)
                .await?;

        let tags = tag_rows
            .iter()
            .map(|r| ExperimentTag {
                key: r.get("key"),
                value: r.get("value"),
            })
            .collect();

        Ok(Experiment {
            experiment_id: row.get("experiment_id"),
            name: row.get("name"),
            artifact_location: row.get("artifact_location"),
            lifecycle_stage: if row.get::<String, _>("lifecycle_stage") == "active" {
                LifecycleStage::Active
            } else {
                LifecycleStage::Deleted
            },
            creation_time: row.get("creation_time"),
            last_update_time: row.get("last_update_time"),
            tags,
        })
    }

    async fn get_experiment_by_name(&self, name: &str) -> Result<Experiment> {
        let row = sqlx::query(
            "SELECT experiment_id FROM experiments WHERE name = ? AND lifecycle_stage = 'active'",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .context("experiment not found")?;

        self.get_experiment(row.get("experiment_id")).await
    }

    async fn list_experiments(
        &self,
        lifecycle_stage: Option<LifecycleStage>,
    ) -> Result<Vec<Experiment>> {
        let stage = match lifecycle_stage {
            Some(LifecycleStage::Deleted) => "deleted",
            _ => "active",
        };

        let rows = sqlx::query("SELECT experiment_id FROM experiments WHERE lifecycle_stage = ?")
            .bind(stage)
            .fetch_all(&self.pool)
            .await?;

        let mut experiments = Vec::new();
        for row in rows {
            experiments.push(self.get_experiment(row.get("experiment_id")).await?);
        }
        Ok(experiments)
    }

    async fn delete_experiment(&self, experiment_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE experiments SET lifecycle_stage = 'deleted', last_update_time = ? WHERE experiment_id = ?"
        )
        .bind(now).bind(experiment_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn restore_experiment(&self, experiment_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE experiments SET lifecycle_stage = 'active', last_update_time = ? WHERE experiment_id = ?"
        )
        .bind(now).bind(experiment_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn update_experiment(&self, experiment_id: &str, new_name: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE experiments SET name = ?, last_update_time = ? WHERE experiment_id = ?",
        )
        .bind(new_name)
        .bind(now)
        .bind(experiment_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_experiment_tag(&self, experiment_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO experiment_tags (experiment_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(experiment_id, key) DO UPDATE SET value = excluded.value",
        )
        .bind(experiment_id)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
