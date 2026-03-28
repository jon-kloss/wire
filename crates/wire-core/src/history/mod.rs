use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single entry in the request history log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub elapsed_ms: u64,
}
