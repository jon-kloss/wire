use crate::sandbox;
use crate::state::{SessionInner, SessionState, SharedState};
use axum::extract::State;
use axum::http::{header, HeaderMap, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

const COOKIE_NAME: &str = "wire_session";

/// Middleware that attaches a per-visitor session (and sandbox) to every
/// request, creating one on first contact and setting a session cookie.
pub async fn attach_session(
    State(state): State<SharedState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let existing = read_cookie(req.headers(), COOKIE_NAME);
    let is_new = existing.is_none();
    let sid = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let session = ensure_session(&state, &sid).await;
    req.extensions_mut().insert(session);

    let mut res = next.run(req).await;

    if is_new {
        if let Ok(value) = format!("{COOKIE_NAME}={sid}; Path=/; HttpOnly; SameSite=Lax").parse() {
            res.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    res
}

async fn ensure_session(state: &SharedState, sid: &str) -> Arc<SessionState> {
    let mut sessions = state.sessions.lock().await;
    if let Some(session) = sessions.get(sid) {
        return session.clone();
    }

    let root = std::env::temp_dir().join("wire-web").join(sid);
    // If seeding fails we still hand back a session pointed at the (possibly
    // partial) sandbox; handlers will surface any resulting filesystem errors.
    let _ = sandbox::seed_sandbox(&root, &state.demo_base_url);
    let canonical = std::fs::canonicalize(&root).unwrap_or(root);

    let session = Arc::new(SessionState {
        sandbox: canonical,
        inner: Mutex::new(SessionInner::default()),
    });
    sessions.insert(sid.to_string(), session.clone());
    session
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}
