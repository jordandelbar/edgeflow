use crate::TargetStore;
use anyhow::Result;
use edgeflow_core::*;
use sqlx::Row;

use super::SqliteStore;

fn row_to_resource_settings(r: &sqlx::sqlite::SqliteRow) -> ResourceSettings {
    ResourceSettings {
        sessions: r.get("sessions"),
        max_concurrent: r.get("max_concurrent"),
    }
}

#[async_trait::async_trait]
impl TargetStore for SqliteStore {
    async fn ensure_target(&self, target: &str) -> Result<Target> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO targets (target, registered_at) VALUES (?, ?)
             ON CONFLICT(target) DO NOTHING",
        )
        .bind(target)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_target(target).await.map(|t| t.unwrap())
    }

    async fn set_target_model(&self, target: &str, run_id: &str, loaded_at: &str) -> Result<()> {
        sqlx::query("UPDATE targets SET current_run_id = ?, model_loaded_at = ? WHERE target = ?")
            .bind(run_id)
            .bind(loaded_at)
            .bind(target)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn store_target_resources(
        &self,
        target: &str,
        resources: &ResourceSettings,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO targets (target, registered_at, sessions, max_concurrent)
             VALUES (?, 0, ?, ?)
             ON CONFLICT(target) DO UPDATE SET
               sessions       = excluded.sessions,
               max_concurrent = excluded.max_concurrent",
        )
        .bind(target)
        .bind(resources.sessions)
        .bind(resources.max_concurrent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_target(&self, target: &str) -> Result<Option<Target>> {
        let row = sqlx::query(
            "SELECT target, registered_at, sessions, max_concurrent,
                    current_run_id, model_loaded_at
             FROM targets WHERE target = ?",
        )
        .bind(target)
        .fetch_optional(&self.pool)
        .await?;

        let Some(r) = row else { return Ok(None) };

        Ok(Some(Target {
            target: r.get("target"),
            registered_at: r.get("registered_at"),
            resources: row_to_resource_settings(&r),
            infra: None,
            current_run_id: r.get("current_run_id"),
            model_loaded_at: r.get("model_loaded_at"),
            pods: vec![],
            health: TargetHealth::Unknown,
            node: None,
        }))
    }

    async fn list_targets(&self) -> Result<Vec<Target>> {
        let rows = sqlx::query(
            "SELECT target, registered_at, sessions, max_concurrent,
                    current_run_id, model_loaded_at
             FROM targets ORDER BY registered_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| Target {
                target: r.get("target"),
                registered_at: r.get("registered_at"),
                resources: row_to_resource_settings(r),
                infra: None,
                current_run_id: r.get("current_run_id"),
                model_loaded_at: r.get("model_loaded_at"),
                pods: vec![],
                health: TargetHealth::Unknown,
                node: None,
            })
            .collect())
    }

    async fn delete_target(&self, target: &str) -> Result<()> {
        sqlx::query(
            "UPDATE deployments SET state = 'superseded'
             WHERE target = ? AND state NOT IN ('superseded', 'failed')",
        )
        .bind(target)
        .execute(&self.pool)
        .await?;
        sqlx::query("DELETE FROM targets WHERE target = ?")
            .bind(target)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
