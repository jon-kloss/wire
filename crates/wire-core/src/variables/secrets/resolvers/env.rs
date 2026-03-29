use crate::error::WireError;

/// Resolve a secret from process environment variables.
pub fn resolve(key: &str) -> Result<String, WireError> {
    std::env::var(key).map_err(|_| {
        WireError::Other(format!(
            "Secret not found: $env:{key} — environment variable '{key}' is not set"
        ))
    })
}
