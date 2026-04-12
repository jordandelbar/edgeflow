use crate::DeploymentStore;
use anyhow::{Context, Result};
use edgeflow_core::*;
use sqlx::Row;

use super::SqliteStore;

fn row_to_deployment(row: &sqlx::sqlite::SqliteRow) -> Deployment {
    Deployment {
        deployment_id: row.get("deployment_id"),
        target: row.get("target"),
        run_id: row.get("run_id"),
        model_name: row.try_get("model_name").ok().flatten(),
        model_version: row.try_get("model_version").ok().flatten(),
        created_at: row.get("created_at"),
        state: DeploymentState::from_str(&row.get::<String, _>("state")),
    }
}

#[async_trait::async_trait]
impl DeploymentStore for SqliteStore {
    async fn create_deployment(
        &self,
        run_id: &str,
        target: &str,
        model_name: Option<&str>,
        model_version: Option<&str>,
    ) -> Result<Deployment> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO deployments (deployment_id, target, run_id, model_name, model_version, created_at, state) VALUES (?, ?, ?, ?, ?, ?, 'pending')"
        )
        .bind(&id).bind(target).bind(run_id).bind(model_name).bind(model_version).bind(now)
        .execute(&self.pool).await?;

        self.get_deployment(&id).await
    }

    async fn get_deployment(&self, deployment_id: &str) -> Result<Deployment> {
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, model_name, model_version, created_at, state FROM deployments WHERE deployment_id = ?"
        )
        .bind(deployment_id)
        .fetch_one(&self.pool)
        .await
        .context("deployment not found")?;

        Ok(row_to_deployment(&row))
    }

    async fn get_latest_deployment(&self, target: &str) -> Result<Deployment> {
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, model_name, model_version, created_at, state FROM deployments
             WHERE target = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(target)
        .fetch_one(&self.pool)
        .await
        .context("deployment not found")?;

        Ok(row_to_deployment(&row))
    }

    async fn list_deployments(&self, target: Option<&str>) -> Result<Vec<Deployment>> {
        // JOIN with targets so deployments for torn-down targets are excluded.
        let rows = match target {
            Some(t) => sqlx::query(
                "SELECT d.deployment_id, d.target, d.run_id, d.model_name, d.model_version, d.created_at, d.state
                 FROM deployments d
                 INNER JOIN targets t ON t.target = d.target
                 WHERE d.target = ? ORDER BY d.created_at DESC"
            )
            .bind(t)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT d.deployment_id, d.target, d.run_id, d.model_name, d.model_version, d.created_at, d.state
                 FROM deployments d
                 INNER JOIN targets t ON t.target = d.target
                 ORDER BY d.created_at DESC"
            )
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows.iter().map(row_to_deployment).collect())
    }

    async fn update_deployment_state(
        &self,
        deployment_id: &str,
        state: DeploymentState,
    ) -> Result<()> {
        sqlx::query("UPDATE deployments SET state = ? WHERE deployment_id = ?")
            .bind(state.as_str())
            .bind(deployment_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_pending_deployment_for_target(&self, target: &str) -> Result<Option<Deployment>> {
        // Include 'upgrading' so the pod poll loop can pick up MQTT-dispatched
        // upgrades as a fallback when the pod has no MQTT connection.
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, model_name, model_version, created_at, state FROM deployments
             WHERE target = ? AND state IN ('pending', 'deploying', 'upgrading') ORDER BY created_at DESC LIMIT 1"
        )
        .bind(target)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_deployment))
    }

    async fn supersede_previous_deployments(&self, target: &str, except_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE deployments SET state = 'superseded'
             WHERE target = ? AND state = 'deployed' AND deployment_id != ?",
        )
        .bind(target)
        .bind(except_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_stale_deployments(
        &self,
        states: &[&str],
        older_than_ms: i64,
    ) -> Result<Vec<Deployment>> {
        // SQLite doesn't support array parameters; run one query per state.
        let mut results = Vec::new();
        let cutoff = chrono::Utc::now().timestamp_millis() - older_than_ms;
        for state in states {
            let rows = sqlx::query(
                "SELECT deployment_id, target, run_id, model_name, model_version, created_at, state FROM deployments
                 WHERE state = ? AND created_at < ?"
            )
            .bind(state).bind(cutoff)
            .fetch_all(&self.pool).await?;

            for row in &rows {
                results.push(row_to_deployment(row));
            }
        }
        Ok(results)
    }
}
