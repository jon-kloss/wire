use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("{0}")]
    Other(String),
}

// Tauri IPC requires Serialize on error types.
// We serialize the Display representation as a string.
impl Serialize for WireError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
