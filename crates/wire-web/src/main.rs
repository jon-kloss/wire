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
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

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

    let state: SharedState = Arc::new(AppState::new(demo_base_url.clone()));

    let app = Router::new()
        .nest("/api", api_router(state.clone()))
        .nest("/demo", demo::router())
        .fallback_service(
            ServeDir::new(&ui_dir)
                .not_found_service(ServeFile::new(format!("{ui_dir}/index.html"))),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("Wire web playground listening on http://{addr}");
    tracing::info!("Demo API base URL: {demo_base_url}");
    tracing::info!("Serving UI from: {ui_dir}");

    axum::serve(listener, app).await.expect("server error");
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
        .layer(from_fn_with_state(state, session::attach_session))
}
