use edgeflow_store::sqlite::SqliteStore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub artifact_root: PathBuf,
    pub http_client: reqwest::Client,
}
