use crate::error::WireError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A saved API response snapshot (golden file).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Value,
}

/// Compute the snapshot file path for a given request path.
///
/// Mirrors the request directory structure under `.wire/snapshots/`.
/// For example, `requests/users/list.wire.yaml` -> `snapshots/users/list.snapshot.json`.
pub fn snapshot_path(wire_dir: &Path, request_relative_path: &str) -> PathBuf {
    let clean = request_relative_path
        .trim_start_matches("requests/")
        .trim_start_matches("requests\\")
        .trim_end_matches(".wire.yaml");
    wire_dir
        .join("snapshots")
        .join(format!("{clean}.snapshot.json"))
}

/// Save a snapshot to disk as canonical JSON (sorted keys, pretty-printed).
pub fn save_snapshot(
    snapshot: &Snapshot,
    wire_dir: &Path,
    request_relative_path: &str,
) -> Result<PathBuf, WireError> {
    let path = snapshot_path(wire_dir, request_relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Canonical JSON: sorted keys via serde_json with sorted map feature
    let json = canonical_json(snapshot)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Load a snapshot from disk. Returns None if the file doesn't exist.
pub fn load_snapshot(
    wire_dir: &Path,
    request_relative_path: &str,
) -> Result<Option<Snapshot>, WireError> {
    let path = snapshot_path(wire_dir, request_relative_path);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let snapshot: Snapshot =
        serde_json::from_str(&content).map_err(|e| WireError::Parse(e.to_string()))?;
    Ok(Some(snapshot))
}

/// Create a Snapshot from an HTTP response.
pub fn snapshot_from_response(
    status: u16,
    headers: &HashMap<String, String>,
    body: &str,
) -> Snapshot {
    let body_value = serde_json::from_str(body).unwrap_or(Value::String(body.to_string()));

    // Only keep content-type header
    let mut snapshot_headers = HashMap::new();
    for (k, v) in headers {
        if k.to_lowercase() == "content-type" {
            snapshot_headers.insert(k.clone(), v.clone());
        }
    }

    Snapshot {
        status,
        headers: snapshot_headers,
        body: body_value,
    }
}

/// Serialize a snapshot as canonical JSON with sorted keys.
fn canonical_json(snapshot: &Snapshot) -> Result<String, WireError> {
    // Serialize to Value first to ensure sorted keys
    let value = serde_json::to_value(snapshot).map_err(|e| WireError::Parse(e.to_string()))?;
    let sorted = sort_value(&value);
    serde_json::to_string_pretty(&sorted).map_err(|e| WireError::Parse(e.to_string()))
}

/// Recursively sort all object keys in a JSON value.
fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), sort_value(v)))
                .collect();
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn snapshot_path_from_request() {
        let path = snapshot_path(Path::new(".wire"), "requests/users/list.wire.yaml");
        assert_eq!(
            path,
            PathBuf::from(".wire/snapshots/users/list.snapshot.json")
        );
    }

    #[test]
    fn snapshot_path_nested() {
        let path = snapshot_path(Path::new(".wire"), "requests/api/v2/users/get.wire.yaml");
        assert_eq!(
            path,
            PathBuf::from(".wire/snapshots/api/v2/users/get.snapshot.json")
        );
    }

    #[test]
    fn snapshot_from_json_response() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-request-id".to_string(), "abc123".to_string());

        let snap = snapshot_from_response(200, &headers, r#"{"name":"Alice"}"#);

        assert_eq!(snap.status, 200);
        assert_eq!(snap.body, json!({"name": "Alice"}));
        // Only content-type kept
        assert_eq!(snap.headers.len(), 1);
        assert!(snap.headers.contains_key("content-type"));
    }

    #[test]
    fn snapshot_from_non_json_response() {
        let headers = HashMap::new();
        let snap = snapshot_from_response(200, &headers, "plain text body");
        assert_eq!(snap.body, json!("plain text body"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = dir.path();

        let snap = Snapshot {
            status: 200,
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h
            },
            body: json!({"users": [{"name": "Alice"}, {"name": "Bob"}]}),
        };

        let path = save_snapshot(&snap, wire_dir, "requests/users/list.wire.yaml").unwrap();
        assert!(path.exists());

        let loaded = load_snapshot(wire_dir, "requests/users/list.wire.yaml")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, 200);
        assert_eq!(loaded.body, snap.body);
    }

    #[test]
    fn load_missing_snapshot_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_snapshot(dir.path(), "requests/nonexistent.wire.yaml").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn canonical_json_has_sorted_keys() {
        let snap = Snapshot {
            status: 200,
            headers: HashMap::new(),
            body: json!({"zebra": 1, "alpha": 2}),
        };
        let json = canonical_json(&snap).unwrap();
        let alpha_pos = json.find("alpha").unwrap();
        let zebra_pos = json.find("zebra").unwrap();
        assert!(
            alpha_pos < zebra_pos,
            "keys should be sorted alphabetically"
        );
    }

    #[test]
    fn snapshot_config_deserializes() {
        use crate::collection::SnapshotConfig;
        let yaml = r#"
ignore:
  - body.timestamp
  - body.users[*].id
"#;
        let config: SnapshotConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.ignore.len(), 2);
        assert_eq!(config.ignore[0], "body.timestamp");
    }

    #[test]
    fn wire_request_with_snapshot_config() {
        use crate::collection::WireRequest;
        let yaml = r#"
name: List Users
method: GET
url: http://localhost/api/users
snapshot:
  ignore:
    - body.timestamp
"#;
        let req: WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert!(req.snapshot.is_some());
        assert_eq!(req.snapshot.unwrap().ignore, vec!["body.timestamp"]);
    }
}
