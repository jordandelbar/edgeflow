use crate::Store;
use anyhow::{Context, Result};
use edgeflow_core::*;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parse `tag.\`key\` = 'value'` or `tag.'key' = 'value'` into `(key, value)`.
fn parse_tag_filter(filter: &str) -> Option<(String, String)> {
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
impl Store for SqliteStore {
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
        let row = sqlx::query(
            "SELECT deployment_id, target, run_id, model_name, model_version, created_at, state FROM deployments
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

    // ── Model Registry ────────────────────────────────────────────────────────

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

    // ── Targets ───────────────────────────────────────────────────────────────

    async fn register_pod(
        &self,
        pod_id: &str,
        target: &str,
        address: &str,
        node: Option<&str>,
    ) -> Result<Target> {
        let now = chrono::Utc::now().timestamp_millis();

        // Ensure the target record exists; do not overwrite registered_at if already set.
        sqlx::query(
            "INSERT INTO targets (target, registered_at) VALUES (?, ?)
             ON CONFLICT(target) DO NOTHING",
        )
        .bind(target)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Upsert the pod record. Node is backfilled if currently NULL.
        sqlx::query(
            "INSERT INTO target_pods (pod_id, target, address, node, registered_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(pod_id) DO UPDATE SET
               address       = excluded.address,
               node          = COALESCE(target_pods.node, excluded.node),
               registered_at = excluded.registered_at",
        )
        .bind(pod_id)
        .bind(target)
        .bind(address)
        .bind(node)
        .bind(now)
        .execute(&self.pool)
        .await?;

        self.get_target(target).await.map(|t| t.unwrap())
    }

    async fn store_target_resources(
        &self,
        target: &str,
        resources: &ResourceSettings,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO targets (target, registered_at, cpu_request, memory_request, memory_limit, sessions, max_concurrent)
             VALUES (?, 0, ?, ?, ?, ?, ?)
             ON CONFLICT(target) DO UPDATE SET
               cpu_request    = excluded.cpu_request,
               memory_request = excluded.memory_request,
               memory_limit   = excluded.memory_limit,
               sessions       = excluded.sessions,
               max_concurrent = excluded.max_concurrent",
        )
        .bind(target)
        .bind(&resources.cpu_request)
        .bind(&resources.memory_request)
        .bind(&resources.memory_limit)
        .bind(resources.sessions)
        .bind(resources.max_concurrent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn heartbeat_pod(&self, pod_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE target_pods SET last_seen = ? WHERE pod_id = ?")
            .bind(now)
            .bind(pod_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    async fn get_target(&self, target: &str) -> Result<Option<Target>> {
        let row = sqlx::query(
            "SELECT target, registered_at, cpu_request, memory_request, memory_limit,
                    sessions, max_concurrent, current_run_id, model_loaded_at
             FROM targets WHERE target = ?",
        )
        .bind(target)
        .fetch_optional(&self.pool)
        .await?;

        let Some(r) = row else { return Ok(None) };

        let pod_rows = sqlx::query(
            "SELECT pod_id, address, node, registered_at, last_seen
             FROM target_pods WHERE target = ? ORDER BY registered_at ASC",
        )
        .bind(target)
        .fetch_all(&self.pool)
        .await?;

        let pods = rows_to_pods(pod_rows);
        let health = TargetHealth::aggregate(&pods);
        let node = pods.first().and_then(|p| p.node.clone());
        let last_seen = pods.iter().filter_map(|p| p.last_seen).max();

        Ok(Some(Target {
            target: r.get("target"),
            registered_at: r.get("registered_at"),
            resources: ResourceSettings {
                cpu_request: r.get("cpu_request"),
                memory_request: r.get("memory_request"),
                memory_limit: r.get("memory_limit"),
                sessions: r.get("sessions"),
                max_concurrent: r.get("max_concurrent"),
            },
            current_run_id: r.get("current_run_id"),
            model_loaded_at: r.get("model_loaded_at"),
            pods,
            health,
            node,
            last_seen,
        }))
    }

    async fn list_targets(&self) -> Result<Vec<Target>> {
        let target_rows = sqlx::query(
            "SELECT target, registered_at, cpu_request, memory_request, memory_limit,
                    sessions, max_concurrent, current_run_id, model_loaded_at
             FROM targets ORDER BY registered_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let pod_rows = sqlx::query(
            "SELECT pod_id, target, address, node, registered_at, last_seen
             FROM target_pods ORDER BY registered_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        // Group pods by target.
        let mut pods_by_target: HashMap<String, Vec<TargetPod>> = HashMap::new();
        for r in pod_rows {
            let target_name: String = r.get("target");
            let last_seen: Option<i64> = r.get("last_seen");
            let pod = TargetPod {
                pod_id: r.get("pod_id"),
                address: r.get("address"),
                node: r.get("node"),
                registered_at: r.get("registered_at"),
                last_seen,
                health: TargetHealth::from_last_seen(last_seen),
            };
            pods_by_target.entry(target_name).or_default().push(pod);
        }

        Ok(target_rows
            .iter()
            .map(|r| {
                let target_name: String = r.get("target");
                let pods = pods_by_target.remove(&target_name).unwrap_or_default();
                let health = TargetHealth::aggregate(&pods);
                let node = pods.first().and_then(|p| p.node.clone());
                let last_seen = pods.iter().filter_map(|p| p.last_seen).max();
                Target {
                    target: target_name,
                    registered_at: r.get("registered_at"),
                    resources: ResourceSettings {
                        cpu_request: r.get("cpu_request"),
                        memory_request: r.get("memory_request"),
                        memory_limit: r.get("memory_limit"),
                        sessions: r.get("sessions"),
                        max_concurrent: r.get("max_concurrent"),
                    },
                    current_run_id: r.get("current_run_id"),
                    model_loaded_at: r.get("model_loaded_at"),
                    pods,
                    health,
                    node,
                    last_seen,
                }
            })
            .collect())
    }

    async fn delete_target(&self, target: &str) -> Result<()> {
        // Supersede any active deployments first so history is preserved.
        sqlx::query(
            "UPDATE deployments SET state = 'superseded'
             WHERE target = ? AND state NOT IN ('superseded', 'failed')",
        )
        .bind(target)
        .execute(&self.pool)
        .await?;

        // ON DELETE CASCADE in target_pods handles pod row cleanup.
        sqlx::query("DELETE FROM targets WHERE target = ?")
            .bind(target)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

fn rows_to_pods(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<TargetPod> {
    rows.into_iter()
        .map(|r| {
            let last_seen: Option<i64> = r.get("last_seen");
            TargetPod {
                pod_id: r.get("pod_id"),
                address: r.get("address"),
                node: r.get("node"),
                registered_at: r.get("registered_at"),
                last_seen,
                health: TargetHealth::from_last_seen(last_seen),
            }
        })
        .collect()
}
