use std::path::PathBuf;
use std::sync::Arc;
use edgeflow_store::sqlite::SqliteStore;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub artifact_root: PathBuf,
}
