use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wire_core::collection::{Assertion, WireRequest};
use wire_core::http::WireResponse;

// ---------------------------------------------------------------------------
// Response payloads (mirror crates/wire-app/src/types.rs so the existing React
// UI deserializes them unchanged).
// ---------------------------------------------------------------------------

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

impl From<IpcResponse> for WireResponse {
    fn from(r: IpcResponse) -> Self {
        WireResponse {
            status: r.status,
            status_text: r.status_text,
            headers: r.headers,
            body: r.body,
            elapsed: std::time::Duration::from_millis(r.elapsed_ms),
            size_bytes: r.size_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequestEntry {
    pub path: String,
    pub name: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcCollectionInfo {
    pub name: String,
    pub version: u32,
    pub active_env: Option<String>,
    pub default_templates: Vec<String>,
    pub requests: Vec<IpcRequestEntry>,
    pub environments: Vec<String>,
    pub templates: Vec<String>,
    pub source_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcScanResult {
    pub framework: String,
    pub endpoints_found: usize,
    pub files_scanned: usize,
    pub collection: Option<IpcCollectionInfo>,
    pub wire_dir: Option<String>,
}

/// A seeded sample available in the playground, returned by `list_samples`.
/// The web UI uses these to replace the desktop folder picker.
#[derive(Debug, Clone, Serialize)]
pub struct SampleInfo {
    pub label: String,
    pub description: String,
    /// Sandbox-absolute path the UI echoes back to the relevant command.
    pub path: String,
    /// "collection" or "project".
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Request argument payloads. `rename_all = "camelCase"` matches what the React
// UI sends today through Tauri's `invoke` (which lowercases JS camelCase keys).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCollectionArgs {
    pub wire_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestArgs {
    pub file: String,
    #[serde(default)]
    pub env: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRawArgs {
    pub request: WireRequest,
    #[serde(default)]
    pub env: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListHistoryArgs {
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionArgs {
    pub name: String,
    pub parent_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameCollectionArgs {
    pub wire_dir: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanArgs {
    pub project_dir: String,
    pub output_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEnvArgs {
    pub wire_dir: String,
    pub env_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEnvArgs {
    pub wire_dir: String,
    pub env_name: String,
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadRequestArgs {
    pub file: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTemplatesArgs {
    pub wire_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameArgs {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequestArgs {
    pub path: String,
    pub request: WireRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTemplateArgs {
    pub name: String,
    pub request: WireRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleDefaultArgs {
    pub wire_dir: String,
    pub template: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunChainArgs {
    pub file: String,
    #[serde(default)]
    pub env: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateTestsArgs {
    pub assertions: Vec<Assertion>,
    pub response: IpcResponse,
}
