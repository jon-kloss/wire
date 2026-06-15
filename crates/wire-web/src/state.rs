use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use wire_core::collection::LoadedCollection;
use wire_core::http::HttpClient;

/// Shared application state, cloned (via Arc) into every handler.
pub type SharedState = Arc<AppState>;

pub struct AppState {
    /// Shared HTTP client used to execute requests against the demo API.
    pub http_client: HttpClient,
    /// Base URL of the bundled demo API (the only allowed request target).
    /// Seeded environment files substitute this in for `{{base_url}}`.
    pub demo_base_url: String,
    /// Per-visitor sessions, keyed by the `wire_session` cookie.
    pub sessions: Mutex<HashMap<String, Arc<SessionState>>>,
    /// In-memory data backing the bundled demo API (shared across sessions).
    pub demo_pets: Mutex<Vec<serde_json::Value>>,
}

impl AppState {
    pub fn new(demo_base_url: String) -> Self {
        Self {
            http_client: HttpClient::default(),
            demo_base_url,
            sessions: Mutex::new(HashMap::new()),
            demo_pets: Mutex::new(vec![
                serde_json::json!({ "id": 1, "name": "Fido", "species": "dog" }),
                serde_json::json!({ "id": 2, "name": "Whiskers", "species": "cat" }),
            ]),
        }
    }
}

/// State for a single visitor session. Each session owns an isolated sandbox
/// directory on disk, seeded with sample collections and projects.
pub struct SessionState {
    /// Canonicalized root of this session's sandbox. All client-supplied paths
    /// are resolved against and confined to this directory.
    pub sandbox: PathBuf,
    pub inner: Mutex<SessionInner>,
}

#[derive(Default)]
pub struct SessionInner {
    pub collection: Option<LoadedCollection>,
    pub collection_path: Option<PathBuf>,
}
