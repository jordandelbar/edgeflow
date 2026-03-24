use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};
use edgeflow_core::*;
use crate::Store;

pub struct SqliteStore {
    pool: SqlitePool,
    artifact_root: PathBuf,
}

impl SqliteStore {
    pub async fn new(db_path: &Path, artifact_root: PathBuf) -> Result<Self> {
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&url)
            .await
            .context("failed to open sqlite database")?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .context("failed to run migrations")?;

        Ok(Self { pool, artifact_root })
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    async fn create_experiment(&self, name: &str, artifact_location: Option<&str>, tags: Vec<ExperimentTag>) -> Result<Experiment> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let location = artifact_location
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.artifact_root.join(&id).display().to_string());

        sqlx::query(
            "INSERT INTO experiments (experiment_id, name, artifact_location, lifecycle_stage, creation_time, last_update_time)
             VALUES (?, ?, ?, 'active', ?, ?)"
        )
        .bind(&id).bind(name).bind(&location).bind(now).bind(now)
        .execute(&self.pool).await?;

        for tag in &tags {
            sqlx::query("INSERT INTO experiment_tags (experiment_id, key, value) VALUES (?, ?, ?)")
                .bind(&id).bind(&tag.key).bind(&tag.value)
                .execute(&self.pool).await?;
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

        let tag_rows = sqlx::query("SELECT key, value FROM experiment_tags WHERE experiment_id = ?")
            .bind(experiment_id)
            .fetch_all(&self.pool)
            .await?;

        let tags = tag_rows.iter().map(|r| ExperimentTag {
            key: r.get("key"),
            value: r.get("value"),
        }).collect();

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
            "SELECT experiment_id FROM experiments WHERE name = ? AND lifecycle_stage = 'active'"
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .context("experiment not found")?;

        self.get_experiment(row.get("experiment_id")).await
    }

    async fn list_experiments(&self, lifecycle_stage: Option<LifecycleStage>) -> Result<Vec<Experiment>> {
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
        sqlx::query("UPDATE experiments SET name = ?, last_update_time = ? WHERE experiment_id = ?")
            .bind(new_name).bind(now).bind(experiment_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn set_experiment_tag(&self, experiment_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO experiment_tags (experiment_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(experiment_id, key) DO UPDATE SET value = excluded.value"
        )
        .bind(experiment_id).bind(key).bind(value)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn create_run(&self, experiment_id: &str, run_name: Option<&str>, start_time: Option<i64>, tags: Vec<RunTag>) -> Result<Run> {
        let run_id = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let start = start_time.unwrap_or(now);
        let exp = self.get_experiment(experiment_id).await?;
        let artifact_uri = format!("{}/{}", exp.artifact_location, run_id);

        sqlx::query(
            "INSERT INTO runs (run_id, experiment_id, run_name, status, start_time, artifact_uri, lifecycle_stage)
             VALUES (?, ?, ?, 'RUNNING', ?, ?, 'active')"
        )
        .bind(&run_id).bind(experiment_id).bind(run_name).bind(start).bind(&artifact_uri)
        .execute(&self.pool).await?;

        for tag in &tags {
            sqlx::query(
                "INSERT INTO run_tags (run_id, key, value) VALUES (?, ?, ?)
                 ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value"
            )
            .bind(&run_id).bind(&tag.key).bind(&tag.value)
            .execute(&self.pool).await?;
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
            "SELECT key, value, timestamp, step FROM metrics WHERE run_id = ? ORDER BY step ASC"
        )
        .bind(run_id).fetch_all(&self.pool).await?;

        let param_rows = sqlx::query("SELECT key, value FROM params WHERE run_id = ?")
            .bind(run_id).fetch_all(&self.pool).await?;

        let tag_rows = sqlx::query("SELECT key, value FROM run_tags WHERE run_id = ?")
            .bind(run_id).fetch_all(&self.pool).await?;

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
                metrics: metric_rows.iter().map(|r| Metric {
                    key: r.get("key"),
                    value: r.get("value"),
                    timestamp: r.get("timestamp"),
                    step: r.get("step"),
                }).collect(),
                params: param_rows.iter().map(|r| Param {
                    key: r.get("key"),
                    value: r.get("value"),
                }).collect(),
                tags: tag_rows.iter().map(|r| RunTag {
                    key: r.get("key"),
                    value: r.get("value"),
                }).collect(),
            },
        })
    }

    async fn update_run(&self, run_id: &str, status: RunStatus, end_time: Option<i64>, run_name: Option<&str>) -> Result<RunInfo> {
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
            .bind(run_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn restore_run(&self, run_id: &str) -> Result<()> {
        sqlx::query("UPDATE runs SET lifecycle_stage = 'active' WHERE run_id = ?")
            .bind(run_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn search_runs(&self, experiment_ids: Vec<String>, _filter: Option<&str>, max_results: i64) -> Result<Vec<Run>> {
        let placeholders = experiment_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT run_id FROM runs WHERE experiment_id IN ({}) AND lifecycle_stage = 'active' LIMIT ?",
            placeholders
        );
        let mut q = sqlx::query(&sql);
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

    async fn log_metric(&self, run_id: &str, metric: Metric) -> Result<()> {
        sqlx::query("INSERT INTO metrics (run_id, key, value, timestamp, step) VALUES (?, ?, ?, ?, ?)")
            .bind(run_id).bind(&metric.key).bind(metric.value).bind(metric.timestamp).bind(metric.step)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn log_batch(&self, run_id: &str, metrics: Vec<Metric>, params: Vec<Param>, tags: Vec<RunTag>) -> Result<()> {
        for m in metrics { self.log_metric(run_id, m).await?; }
        for p in params { self.log_param(run_id, &p.key, &p.value).await?; }
        for t in tags { self.set_tag(run_id, &t.key, &t.value).await?; }
        Ok(())
    }

    async fn log_param(&self, run_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO params (run_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value"
        )
        .bind(run_id).bind(key).bind(value)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn set_tag(&self, run_id: &str, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO run_tags (run_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(run_id, key) DO UPDATE SET value = excluded.value"
        )
        .bind(run_id).bind(key).bind(value)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_metric_history(&self, run_id: &str, metric_key: &str) -> Result<Vec<Metric>> {
        let rows = sqlx::query(
            "SELECT key, value, timestamp, step FROM metrics WHERE run_id = ? AND key = ? ORDER BY step ASC"
        )
        .bind(run_id).bind(metric_key)
        .fetch_all(&self.pool).await?;

        Ok(rows.iter().map(|r| Metric {
            key: r.get("key"),
            value: r.get("value"),
            timestamp: r.get("timestamp"),
            step: r.get("step"),
        }).collect())
    }

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
                let relative = entry.path()
                    .strip_prefix(&self.artifact_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| entry.path().display().to_string());
                files.push(FileInfo {
                    path: relative,
                    is_dir: meta.is_dir(),
                    file_size: if meta.is_file() { Some(meta.len() as i64) } else { None },
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
        Ok(PathBuf::from(row.get::<String, _>("artifact_uri")))
    }
}
