use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub key: String,
    pub value: f64,
    pub timestamp: i64,
    pub step: i64,
}
