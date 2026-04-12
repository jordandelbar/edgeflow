use crate::MetricStore;
use anyhow::Result;
use edgeflow_core::*;
use sqlx::Row;

use super::SqliteStore;

#[async_trait::async_trait]
impl MetricStore for SqliteStore {
    async fn log_metric(&self, run_id: &str, metric: Metric) -> Result<()> {
        sqlx::query(
            "INSERT INTO metrics (run_id, key, value, timestamp, step) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(&metric.key)
        .bind(metric.value)
        .bind(metric.timestamp)
        .bind(metric.step)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn log_batch(
        &self,
        run_id: &str,
        metrics: Vec<Metric>,
        params: Vec<Param>,
        tags: Vec<RunTag>,
    ) -> Result<()> {
        for m in metrics {
            self.log_metric(run_id, m).await?;
        }
        for p in params {
            self.log_param(run_id, &p.key, &p.value).await?;
        }
        for t in tags {
            self.set_tag(run_id, &t.key, &t.value).await?;
        }
        Ok(())
    }

    async fn log_param(&self, run_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO params (run_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value",
        )
        .bind(run_id)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_tag(&self, run_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO run_tags (run_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value",
        )
        .bind(run_id)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_metric_history(&self, run_id: &str, metric_key: &str) -> Result<Vec<Metric>> {
        let rows = sqlx::query(
            "SELECT key, value, timestamp, step FROM metrics WHERE run_id = ? AND key = ? ORDER BY step ASC"
        )
        .bind(run_id).bind(metric_key)
        .fetch_all(&self.pool).await?;

        Ok(rows
            .iter()
            .map(|r| Metric {
                key: r.get("key"),
                value: r.get("value"),
                timestamp: r.get("timestamp"),
                step: r.get("step"),
            })
            .collect())
    }
}
