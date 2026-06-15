mod commands;
mod demo;
mod error;
mod sandbox;
mod session;
mod state;
mod types;

use axum::middleware::from_fn_with_state;
use axum::routing::post;
use axum::Router;
use state::{AppState, SharedState};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wire_web=info,tower_http=info".into()),
        )
        .init();

    let addr = std::env::var("WIRE_WEB_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let port = addr.rsplit(':').next().unwrap_or("8787").to_string();
    // reqwest runs inside this process, so it always reaches the demo API over
    // the loopback address regardless of the public bind host.
    let demo_base_url = std::env::var("WIRE_WEB_DEMO_URL")
        .unwrap_or_else(|_| format!("http://127.0.0.1:{port}/demo"));
    let ui_dir = std::env::var("WIRE_WEB_UI_DIR").unwrap_or_else(|_| "ui/dist".to_string());
    let secure_cookies = env_flag("WIRE_WEB_SECURE_COOKIE", false);
    let session_ttl = Duration::from_secs(env_u64("WIRE_WEB_SESSION_TTL_SECS", 3600));

    warn_if_ui_missing(&ui_dir);

    let state: SharedState = Arc::new(AppState::new(
        demo_base_url.clone(),
        secure_cookies,
        session_ttl,
    ));

    spawn_session_sweeper(state.clone());

    let app = build_app(state, &ui_dir);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("Wire web playground listening on http://{addr}");
    tracing::info!("Demo API base URL: {demo_base_url}");
    tracing::info!("Serving UI from: {ui_dir}");

    axum::serve(listener, app).await.expect("server error");
}

/// Assemble the full application: API routes behind the session middleware, the
/// bundled demo API, and the static UI with SPA fallback.
fn build_app(state: SharedState, ui_dir: &str) -> Router {
    let index = Path::new(ui_dir).join("index.html");
    Router::new()
        .nest("/api", api_router(state.clone()))
        .nest("/demo", demo::router())
        .fallback_service(ServeDir::new(ui_dir).not_found_service(ServeFile::new(index)))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// All command endpoints, behind the session middleware that gives each visitor
/// an isolated sandbox.
fn api_router(state: SharedState) -> Router<SharedState> {
    Router::new()
        .route("/list_samples", post(commands::list_samples))
        .route("/open_collection", post(commands::open_collection))
        .route("/send_request", post(commands::send_request))
        .route("/send_raw_request", post(commands::send_raw_request))
        .route("/list_environments", post(commands::list_environments))
        .route("/list_history", post(commands::list_history))
        .route("/clear_history", post(commands::clear_history))
        .route(
            "/create_collection_cmd",
            post(commands::create_collection_cmd),
        )
        .route(
            "/rename_collection_cmd",
            post(commands::rename_collection_cmd),
        )
        .route("/scan_codebase", post(commands::scan_codebase))
        .route("/get_environment", post(commands::get_environment))
        .route("/save_environment", post(commands::save_environment))
        .route("/read_request", post(commands::read_request))
        .route("/save_request", post(commands::save_request))
        .route("/evaluate_tests", post(commands::evaluate_tests))
        .route("/list_templates_cmd", post(commands::list_templates_cmd))
        .route("/read_template", post(commands::read_template))
        .route("/save_template", post(commands::save_template))
        .route("/delete_template", post(commands::delete_template))
        .route("/check_drift", post(commands::check_drift))
        .route("/fix_drift", post(commands::fix_drift))
        .route(
            "/toggle_default_template",
            post(commands::toggle_default_template),
        )
        .route("/run_chain", post(commands::run_chain))
        .route("/list_source_files", post(commands::list_source_files))
        .route("/read_source_file", post(commands::read_source_file))
        .route("/save_source_file", post(commands::save_source_file))
        .layer(from_fn_with_state(state, session::attach_session))
}

/// Periodically evict idle sessions and delete their sandboxes.
fn spawn_session_sweeper(state: SharedState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            let swept = state.sweep_expired_sessions().await;
            if swept > 0 {
                tracing::info!("swept {swept} expired session(s)");
            }
        }
    });
}

fn warn_if_ui_missing(ui_dir: &str) {
    if !Path::new(ui_dir).join("index.html").is_file() {
        tracing::warn!(
            "UI not found at '{ui_dir}/index.html' — the server will return 404s for the app. \
             Build it with `cd ui && npm run build`, or set WIRE_WEB_UI_DIR."
        );
    }
}

fn env_flag(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> SharedState {
        Arc::new(AppState::new(
            "http://127.0.0.1:9/demo".to_string(),
            false,
            Duration::from_secs(3600),
        ))
    }

    fn post(uri: &str, sid: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(sid) = sid {
            builder = builder.header("cookie", format!("wire_session={sid}"));
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    async fn body_string(resp: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn demo_pets_are_isolated_per_session() {
        let app = build_app(test_state(), "ui/dist");
        // Create a pet in session aaaa.
        let created = app
            .clone()
            .oneshot(post("/demo/s/aaaa/pets", None, r#"{"name":"Rex"}"#))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);

        // A different session must not see it.
        let other = body_string(app.clone().oneshot(get("/demo/s/bbbb/pets")).await.unwrap()).await;
        assert!(
            !other.contains("Rex"),
            "pet leaked across sessions: {other}"
        );

        // The originating session must.
        let mine = body_string(app.oneshot(get("/demo/s/aaaa/pets")).await.unwrap()).await;
        assert!(mine.contains("Rex"), "own pet missing: {mine}");
    }

    #[tokio::test]
    async fn sweep_drops_orphan_demo_stores() {
        let state = test_state();
        let app = build_app(state.clone(), "ui/dist");
        // Allocate a demo store for a sid that has no backing session.
        app.oneshot(get("/demo/s/orphan/pets")).await.unwrap();
        assert!(state.demo_pets.lock().await.contains_key("orphan"));
        // The sweep should drop it (it isn't tied to a live session).
        state.sweep_expired_sessions().await;
        assert!(!state.demo_pets.lock().await.contains_key("orphan"));
    }

    #[tokio::test]
    async fn external_url_is_blocked_by_egress() {
        let app = build_app(test_state(), "ui/dist");
        let resp = app
            .oneshot(post(
                "/api/send_raw_request",
                None,
                r#"{"request":{"name":"x","method":"GET","url":"http://example.com/"}}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_string(resp).await.contains("blocked"));
    }

    #[tokio::test]
    async fn path_escaping_sandbox_is_rejected() {
        let app = build_app(test_state(), "ui/dist");
        let resp = app
            .oneshot(post(
                "/api/open_collection",
                None,
                r#"{"wireDir":"../../etc"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_string(resp).await.contains("escapes"));
    }

    #[tokio::test]
    async fn source_file_traversal_is_rejected() {
        let app = build_app(test_state(), "ui/dist");
        let resp = app
            .oneshot(post(
                "/api/read_source_file",
                None,
                r#"{"sourceDir":"projects/fastapi-api","path":"../../../../etc/passwd"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_string(resp).await.contains("escapes"));
    }

    #[tokio::test]
    async fn source_read_cannot_escape_into_another_project() {
        // Stays inside the sandbox but reaches a different sample — must be
        // rejected by the source-dir confinement, not just the sandbox check.
        let app = build_app(test_state(), "ui/dist");
        let resp = app
            .oneshot(post(
                "/api/read_source_file",
                Some("crossproj"),
                r#"{"sourceDir":"projects/fastapi-api","path":"../express-api/package.json"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_string(resp).await.contains("escapes the project"));
    }

    #[tokio::test]
    async fn lists_and_reads_seeded_source_files() {
        let app = build_app(test_state(), "ui/dist");
        let listed = body_string(
            app.clone()
                .oneshot(post(
                    "/api/list_source_files",
                    Some("srctest"),
                    r#"{"sourceDir":"projects/fastapi-api"}"#,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert!(listed.contains("routers/products.py"), "files: {listed}");
        // The playground's own metadata file must not be exposed as source.
        assert!(
            !listed.contains(".sample.json"),
            "leaked manifest: {listed}"
        );

        let content = body_string(
            app.oneshot(post(
                "/api/read_source_file",
                Some("srctest"),
                r#"{"sourceDir":"projects/fastapi-api","path":"routers/products.py"}"#,
            ))
            .await
            .unwrap(),
        )
        .await;
        assert!(content.contains("APIRouter"), "content: {content}");
    }

    #[tokio::test]
    async fn sessions_get_isolated_sandboxes() {
        let app = build_app(test_state(), "ui/dist");
        let a = body_string(
            app.clone()
                .oneshot(post("/api/list_samples", Some("aaaa"), "{}"))
                .await
                .unwrap(),
        )
        .await;
        let b = body_string(
            app.oneshot(post("/api/list_samples", Some("bbbb"), "{}"))
                .await
                .unwrap(),
        )
        .await;
        assert!(a.contains("/aaaa/"), "session A path missing: {a}");
        assert!(b.contains("/bbbb/"), "session B path missing: {b}");
        assert!(!a.contains("/bbbb/"), "session A leaked B's sandbox");
    }
}
