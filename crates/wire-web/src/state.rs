use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use wire_core::collection::LoadedCollection;
use wire_core::http::HttpClient;

/// Shared application state, cloned (via Arc) into every handler.
pub type SharedState = Arc<AppState>;

pub struct AppState {
    /// Shared HTTP client used to execute requests. Its egress allowlist is what
    /// confines requests to the bundled demo API.
    pub http_client: HttpClient,
    /// Base URL of the bundled demo API. Seeded environment files substitute
    /// this in for `{{base_url}}`.
    pub demo_base_url: String,
    /// Whether to set the `Secure` attribute on the session cookie (enable when
    /// served over HTTPS; off for local `http://` development).
    pub secure_cookies: bool,
    /// Idle lifetime of a session before it (and its sandbox) is swept.
    pub session_ttl: Duration,
    /// Per-visitor sessions, keyed by the `wire_session` cookie.
    pub sessions: Mutex<HashMap<String, Arc<SessionState>>>,
    /// In-memory data backing the bundled demo API (shared across sessions).
    pub demo_pets: Mutex<Vec<serde_json::Value>>,
}

impl AppState {
    pub fn new(demo_base_url: String, secure_cookies: bool, session_ttl: Duration) -> Self {
        Self {
            // Egress is enforced inside the HTTP client itself, so it covers
            // every execution path (single sends and chains) and cannot be
            // bypassed at the command layer.
            http_client: HttpClient::restricted_to(std::slice::from_ref(&demo_base_url))
                .expect("demo base URL must be a valid allowlist URL"),
            demo_base_url,
            secure_cookies,
            session_ttl,
            sessions: Mutex::new(HashMap::new()),
            demo_pets: Mutex::new(seed_demo_pets()),
        }
    }

    /// Remove sessions whose idle time exceeds the TTL, deleting their sandboxes.
    /// Returns the number of sessions swept.
    pub async fn sweep_expired_sessions(&self) -> usize {
        let now = Instant::now();
        let mut sessions = self.sessions.lock().await;
        let mut expired = Vec::new();
        for (sid, session) in sessions.iter() {
            if now.duration_since(*session.last_seen.lock().await) > self.session_ttl {
                expired.push(sid.clone());
            }
        }
        for sid in &expired {
            if let Some(session) = sessions.remove(sid) {
                let _ = std::fs::remove_dir_all(&session.sandbox);
            }
        }
        expired.len()
    }
}

fn seed_demo_pets() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "id": 1, "name": "Fido", "species": "dog" }),
        serde_json::json!({ "id": 2, "name": "Whiskers", "species": "cat" }),
    ]
}

/// State for a single visitor session. Each session owns an isolated sandbox
/// directory on disk, seeded with sample collections and projects.
pub struct SessionState {
    /// Canonicalized root of this session's sandbox. All client-supplied paths
    /// are resolved against and confined to this directory.
    pub sandbox: PathBuf,
    /// Last time a request touched this session, used for idle eviction.
    pub last_seen: Mutex<Instant>,
    pub inner: Mutex<SessionInner>,
}

#[derive(Default)]
pub struct SessionInner {
    pub collection: Option<LoadedCollection>,
    pub collection_path: Option<PathBuf>,
}
