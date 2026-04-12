use crate::{ExperimentStore, RunStore};
use anyhow::{Context, Result};
use edgeflow_core::*;
use sqlx::Row;

use super::{parse_tag_filter, SqliteStore};

#[async_trait::async_trait]
impl RunStore for SqliteStore {
    async fn create_run(
        &self,
        experiment_id: &str,
        run_name: Option<&str>,
        start_time: Option<i64>,
        tags: Vec<RunTag>,
    ) -> Result<Run> {
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let start = start_time.unwrap_or(now);
        let exp = self.get_experiment(experiment_id).await?;
        let artifact_uri = format!("{}/{}/artifacts", exp.artifact_location, run_id);

        sqlx::query(
            "INSERT INTO runs (run_id, experiment_id, run_name, status, start_time, artifact_uri, lifecycle_stage)
             VALUES (?, ?, ?, 'RUNNING', ?, ?, 'active')"
        )
        .bind(&run_id).bind(experiment_id).bind(run_name).bind(start).bind(&artifact_uri)
        .execute(&self.pool).await?;

        for tag in &tags {
            sqlx::query(
                "INSERT INTO run_tags (run_id, key, value) VALUES (?, ?, ?)
                 ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value",
            )
            .bind(&run_id)
            .bind(&tag.key)
            .bind(&tag.value)
            .execute(&self.pool)
            .await?;
        }

        self.get_run(&run_id).await
    }

    async fn get_run(&self, run_id: &str) -> Result<Run> {
        let row = sqlx::query(
            "SELECT run_id, experiment_id, run_name, status, start_time, end_time, artifact_uri, lifecycle_stage
             FROM runs WHERE run_id = ?"
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .context("run not found")?;

        let metric_rows = sqlx::query(
            "SELECT key, value, timestamp, step FROM metrics WHERE run_id = ? ORDER BY step ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        let param_rows = sqlx::query("SELECT key, value FROM params WHERE run_id = ?")
            .bind(run_id)
            .fetch_all(&self.pool)
            .await?;

        let tag_rows = sqlx::query("SELECT key, value FROM run_tags WHERE run_id = ?")
            .bind(run_id)
            .fetch_all(&self.pool)
            .await?;

        let status = match row.get::<String, _>("status").as_str() {
            "RUNNING" => RunStatus::Running,
            "SCHEDULED" => RunStatus::Scheduled,
            "FINISHED" => RunStatus::Finished,
            "FAILED" => RunStatus::Failed,
            _ => RunStatus::Killed,
        };

        Ok(Run {
            info: RunInfo {
                run_id: row.get("run_id"),
                run_uuid: row.get("run_id"),
                experiment_id: row.get("experiment_id"),
                run_name: row.get("run_name"),
                status,
                start_time: row.get("start_time"),
                end_time: row.get("end_time"),
                artifact_uri: row.get("artifact_uri"),
                lifecycle_stage: row.get("lifecycle_stage"),
            },
            data: RunData {
                metrics: metric_rows
                    .iter()
                    .map(|r| Metric {
                        key: r.get("key"),
                        value: r.get("value"),
                        timestamp: r.get("timestamp"),
                        step: r.get("step"),
                    })
                    .collect(),
                params: param_rows
                    .iter()
                    .map(|r| Param {
                        key: r.get("key"),
                        value: r.get("value"),
                    })
                    .collect(),
                tags: tag_rows
                    .iter()
                    .map(|r| RunTag {
                        key: r.get("key"),
                        value: r.get("value"),
                    })
                    .collect(),
            },
        })
    }

    async fn update_run(
        &self,
        run_id: &str,
        status: RunStatus,
        end_time: Option<i64>,
        run_name: Option<&str>,
    ) -> Result<RunInfo> {
        let status_str = match status {
            RunStatus::Running => "RUNNING",
            RunStatus::Scheduled => "SCHEDULED",
            RunStatus::Finished => "FINISHED",
            RunStatus::Failed => "FAILED",
            RunStatus::Killed => "KILLED",
        };
        sqlx::query(
            "UPDATE runs SET status = ?, end_time = COALESCE(?, end_time), run_name = COALESCE(?, run_name) WHERE run_id = ?"
        )
        .bind(status_str).bind(end_time).bind(run_name).bind(run_id)
        .execute(&self.pool).await?;

        Ok(self.get_run(run_id).await?.info)
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        sqlx::query("UPDATE runs SET lifecycle_stage = 'deleted' WHERE run_id = ?")
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn restore_run(&self, run_id: &str) -> Result<()> {
        sqlx::query("UPDATE runs SET lifecycle_stage = 'active' WHERE run_id = ?")
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn search_runs(
        &self,
        experiment_ids: Vec<String>,
        filter: Option<&str>,
        max_results: i64,
    ) -> Result<Vec<Run>> {
        let placeholders = experiment_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        // Parse a single tag filter of the form: tag.`key` = 'value' or tag.'key' = 'value'
        let tag_filter = filter.and_then(parse_tag_filter);

        let sql = if tag_filter.is_some() {
            format!(
                "SELECT r.run_id FROM runs r \
                 JOIN run_tags t ON t.run_id = r.run_id AND t.key = ? AND t.value = ? \
                 WHERE r.experiment_id IN ({}) AND r.lifecycle_stage = 'active' LIMIT ?",
                placeholders
            )
        } else {
            format!(
                "SELECT run_id FROM runs WHERE experiment_id IN ({}) AND lifecycle_stage = 'active' LIMIT ?",
                placeholders
            )
        };

        let mut q = sqlx::query(&sql);
        if let Some((key, value)) = &tag_filter {
            q = q.bind(key).bind(value);
        }
        for id in &experiment_ids {
            q = q.bind(id);
        }
        q = q.bind(max_results);

        let rows = q.fetch_all(&self.pool).await?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(self.get_run(row.get("run_id")).await?);
        }
        Ok(runs)
    }
}
