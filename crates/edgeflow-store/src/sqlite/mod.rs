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

fn row_to_deployment(row: &sqlx::sqlite::SqliteRow) -> Deployment {
    Deployment {
        deployment_id: row.get("deployment_id"),
        target: row.get("target"),
        run_id: row.get("run_id"),
        created_at: row.get("created_at"),
        state: DeploymentState::from_str(&row.get::<String, _>("state")),
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    async fn create_experiment(&self, name: &str, artifact_location: Option<&str>, tags: Vec<ExperimentTag>) -> Result<Experiment> {
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
        let uri: String = row.get("artifact_uri");
        let rel = uri.strip_prefix("mlflow-artifacts:/").unwrap_or(&uri);
        Ok(self.artifact_root.join(rel))
    }

    async fn create_deployment(&self, run_id: &str, target: &str) -> Result<Deployment> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO deployments (deployment_id, target, run_id, created_at, state) VALUES (?, ?, ?, ?, 'pending')"
        )
        .bind(&id).bind(target).bind(run_id).bind(now)
        .execute(&self.pool).await?;

        self.get_deployment(&id).await
    }

    async fn get_deployment(&self, deployment_id: &str) -> Result<Deployment> {
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, created_at, state FROM deployments WHERE deployment_id = ?"
        )
        .bind(deployment_id)
        .fetch_one(&self.pool)
        .await
        .context("deployment not found")?;

        Ok(row_to_deployment(&row))
    }

    async fn list_deployments(&self, target: Option<&str>) -> Result<Vec<Deployment>> {
        let rows = match target {
            Some(t) => sqlx::query(
                "SELECT deployment_id, target, run_id, created_at, state FROM deployments
                 WHERE target = ? ORDER BY created_at DESC"
            )
            .bind(t)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT deployment_id, target, run_id, created_at, state FROM deployments
                 ORDER BY created_at DESC"
            )
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows.iter().map(row_to_deployment).collect())
    }

    async fn get_latest_deployment(&self, target: &str) -> Result<Deployment> {
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, created_at, state FROM deployments
             WHERE target = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(target)
        .fetch_one(&self.pool)
        .await
        .context("deployment not found")?;

        Ok(row_to_deployment(&row))
    }

    async fn update_deployment_state(&self, deployment_id: &str, state: DeploymentState) -> Result<()> {
        sqlx::query("UPDATE deployments SET state = ? WHERE deployment_id = ?")
            .bind(state.as_str()).bind(deployment_id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_pending_deployment_for_target(&self, target: &str) -> Result<Option<Deployment>> {
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, created_at, state FROM deployments
             WHERE target = ? AND state = 'pending' ORDER BY created_at DESC LIMIT 1"
        )
        .bind(target)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_deployment))
    }

    async fn supersede_previous_deployments(&self, target: &str, except_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE deployments SET state = 'superseded'
             WHERE target = ? AND state = 'deployed' AND deployment_id != ?"
        )
        .bind(target).bind(except_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_stale_deployments(&self, states: &[&str], older_than_ms: i64) -> Result<Vec<Deployment>> {
        // SQLite doesn't support array parameters; run one query per state.
        let mut results = Vec::new();
        let cutoff = chrono::Utc::now().timestamp_millis() - older_than_ms;
        for state in states {
            let rows = sqlx::query(
                "SELECT deployment_id, target, run_id, created_at, state FROM deployments
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

    async fn register_target(&self, target: &str, address: &str, pod_name: Option<&str>) -> Result<Target> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO targets (target, address, pod_name, registered_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(target) DO UPDATE SET address = excluded.address, pod_name = excluded.pod_name, registered_at = excluded.registered_at"
        )
        .bind(target).bind(address).bind(pod_name).bind(now)
        .execute(&self.pool).await?;

        Ok(Target {
            target: target.to_string(),
            address: address.to_string(),
            pod_name: pod_name.map(|s| s.to_string()),
            registered_at: now,
        })
    }

    async fn get_target(&self, target: &str) -> Result<Option<Target>> {
        let row = sqlx::query(
            "SELECT target, address, pod_name, registered_at FROM targets WHERE target = ?"
        )
        .bind(target)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Target {
            target: r.get("target"),
            address: r.get("address"),
            pod_name: r.get("pod_name"),
            registered_at: r.get("registered_at"),
        }))
    }
}
