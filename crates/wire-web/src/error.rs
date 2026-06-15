use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Error type for command handlers. Serializes to a plain-text body with a 400
/// status so the frontend `invoke` shim can surface the message the same way
/// the Tauri runtime rejects a command.
pub struct AppError(pub String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.0).into_response()
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError(s.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
