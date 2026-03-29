use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wire_core::http::WireResponse;

/// HTTP response sent to the frontend via IPC.
/// Uses elapsed_ms instead of Duration (which doesn't JSON-serialize cleanly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub elapsed_ms: u64,
    pub size_bytes: usize,
}

impl From<WireResponse> for IpcResponse {
    fn from(r: WireResponse) -> Self {
        Self {
            status: r.status,
            status_text: r.status_text,
            headers: r.headers,
            body: r.body,
            elapsed_ms: r.elapsed.as_millis() as u64,
            size_bytes: r.size_bytes,
        }
    }
}

/// Info about a single request in the collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequestEntry {
    pub path: String,
    pub name: String,
    pub method: String,
}

/// Collection metadata returned to frontend after opening.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcCollectionInfo {
    pub name: String,
    pub version: u32,
    pub active_env: Option<String>,
    pub default_templates: Vec<String>,
    pub requests: Vec<IpcRequestEntry>,
    pub environments: Vec<String>,
    pub templates: Vec<String>,
}

/// Result of scanning a codebase for HTTP endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcScanResult {
    pub framework: String,
    pub endpoints_found: usize,
    pub files_scanned: usize,
    pub collection: Option<IpcCollectionInfo>,
    pub wire_dir: Option<String>,
}
