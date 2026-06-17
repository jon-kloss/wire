//! Contract-accurate mock responses derived from a collection.
//!
//! Given the collection's requests, answer an incoming (method, path) with the
//! saved golden-file **snapshot** when present, else **schema-shaped** JSON,
//! else a minimal `{}`. The mock can't drift from the contract because it *is*
//! the collection (`.wire/` files). The HTTP server lives in the CLI; this
//! module is the pure, testable resolution core.

use crate::collection::WireRequest;
use serde_json::{json, Value};
use std::path::Path;

/// A mock HTTP response.
#[derive(Debug, Clone, PartialEq)]
pub struct MockResponse {
    pub status: u16,
    pub content_type: String,
    pub body: String,
}

/// Split a path into lowercase, slash-trimmed segments for matching.
fn segments(s: &str) -> Vec<String> {
    s.trim_matches('/')
        .to_lowercase()
        .split('/')
        .map(|p| p.to_string())
        .collect()
}

/// An id-like segment: all digits, or a UUID. So a request authored with a
/// concrete example id (`/pets/1`) still mocks any same-kind id (`/pets/42`).
fn id_like(seg: &str) -> bool {
    if !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let parts: Vec<&str> = seg.split('-').collect();
    parts.len() == 5
        && [8usize, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(&n, p)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Score how specifically a route pattern matches a concrete path, or `None`
/// if it doesn't match. Higher = more specific: each exact literal segment
/// scores 2, each id-like literal 1, each `{param}` wildcard 0. Segment counts
/// must match. Used to prefer `/pets/special` over `/pets/{id}` deterministically.
fn match_score(pattern: &str, path: &str) -> Option<i32> {
    let pat = segments(pattern);
    let req = segments(path);
    if pat.len() != req.len() {
        return None;
    }
    let mut score = 0;
    for (a, b) in pat.iter().zip(req.iter()) {
        if a.starts_with('{') && a.ends_with('}') {
            // wildcard: +0
        } else if a == b {
            score += 2;
        } else if id_like(a) && id_like(b) {
            score += 1;
        } else {
            return None;
        }
    }
    Some(score)
}

/// Does a normalized route pattern (`/pets/{id}`) match a concrete path
/// (`/pets/123`)? `{param}` segments are wildcards, literals compare
/// case-insensitively, and an id-like literal also matches any same-kind id.
pub fn route_matches(pattern: &str, path: &str) -> bool {
    match_score(pattern, path).is_some()
}

/// Resolve a mock response for `(method, path)` against the collection's
/// requests, or `None` if nothing matches (the server should 404). Picks the
/// MOST SPECIFIC matching route (literal > id-like > wildcard), breaking ties
/// by route, so results don't depend on directory-read order. `wire_dir` is the
/// `.wire/` directory (for snapshots); request paths are absolute `.wire.yaml` paths.
pub fn resolve(
    requests: &[(std::path::PathBuf, WireRequest)],
    wire_dir: &Path,
    method: &str,
    path: &str,
) -> Option<MockResponse> {
    let mut matches: Vec<(i32, String, &std::path::PathBuf, &WireRequest)> = requests
        .iter()
        .filter(|(_, r)| r.method.eq_ignore_ascii_case(method))
        .filter_map(|(p, r)| {
            let route = crate::drift::normalize_route(&r.url);
            match_score(&route, path).map(|score| (score, route, p, r))
        })
        .collect();
    // Most specific first; tie-break by route for determinism.
    matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    matches
        .first()
        .map(|(_, _, p, r)| build_response(p, r, wire_dir, method))
}

fn build_response(
    req_path: &Path,
    req: &WireRequest,
    wire_dir: &Path,
    method: &str,
) -> MockResponse {
    // Most accurate: a saved snapshot of a real response. Only attempt this
    // when the request path is genuinely inside wire_dir, so a stray absolute
    // path can't make snapshot_path's join() read outside .wire/snapshots/.
    if let Ok(rel) = req_path.strip_prefix(wire_dir) {
        let relative = rel.to_string_lossy().replace('\\', "/");
        if let Ok(Some(snap)) = crate::snapshot::load_snapshot(wire_dir, &relative) {
            let content_type = snap
                .headers
                .get("content-type")
                .cloned()
                .unwrap_or_else(|| "application/json".to_string());
            let body = match &snap.body {
                Value::String(s) => s.clone(),
                v => serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string()),
            };
            return MockResponse {
                status: snap.status,
                content_type,
                body,
            };
        }
    }

    let status = if method.eq_ignore_ascii_case("POST") {
        201
    } else {
        200
    };

    // Next best: shape the response from the declared schema.
    if !req.response_schema.is_empty() {
        return MockResponse {
            status,
            content_type: "application/json".to_string(),
            body: schema_to_json(&req.response_schema),
        };
    }

    // Fallback: an empty object so clients get valid JSON.
    MockResponse {
        status,
        content_type: "application/json".to_string(),
        body: "{}".to_string(),
    }
}

/// Build a sample JSON object from a `response_schema` (field name -> type hint).
pub fn schema_to_json(schema: &[(String, String)]) -> String {
    let mut map = serde_json::Map::new();
    for (name, hint) in schema {
        map.insert(name.clone(), sample_value(hint));
    }
    serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_else(|_| "{}".to_string())
}

/// A representative sample value for a type hint.
fn sample_value(hint: &str) -> Value {
    let h = hint.trim().to_lowercase();
    if h.starts_with("list<") || h.ends_with("[]") || h.starts_with("array") {
        return json!([]);
    }
    match h.as_str() {
        "string" | "string?" | "str" | "text" => json!("string"),
        "int" | "integer" | "long" | "i32" | "i64" | "number" | "double" | "float" | "decimal" => {
            json!(0)
        }
        "bool" | "boolean" => json!(false),
        "guid" | "uuid" => json!("00000000-0000-0000-0000-000000000000"),
        "datetime" | "datetime?" | "date" | "timestamp" => json!("1970-01-01T00:00:00Z"),
        _ => json!(null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::WireRequest;
    use crate::snapshot::{save_snapshot, Snapshot};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn req(method: &str, url: &str, schema: &[(&str, &str)]) -> WireRequest {
        WireRequest {
            name: "r".into(),
            method: method.into(),
            url: url.into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: schema
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            chain: Vec::new(),
            snapshot: None,
        }
    }

    #[test]
    fn route_matches_params_and_literals() {
        assert!(route_matches("/pets/{id}", "/pets/123"));
        assert!(route_matches("/pets/{id}/likes", "/pets/9/likes"));
        assert!(route_matches("/pets", "/pets"));
        assert!(route_matches("/users", "/Users")); // case-insensitive literals
        assert!(!route_matches("/pets/{id}", "/pets")); // segment count
        assert!(!route_matches("/pets", "/pets/123"));
        assert!(!route_matches("/pets/{id}/likes", "/pets/9/comments"));
    }

    #[test]
    fn route_matches_id_like_literals() {
        // A concrete example id matches any same-kind id.
        assert!(route_matches("/pets/1", "/pets/42"));
        assert!(route_matches(
            "/u/123e4567-e89b-12d3-a456-426614174000",
            "/u/00000000-0000-0000-0000-000000000000"
        ));
        // Non-id literals stay strict.
        assert!(!route_matches("/pets/abc", "/pets/42"));
        assert!(!route_matches("/pets/1", "/pets/abc"));
    }

    #[test]
    fn schema_shapes_sample_json() {
        let body = schema_to_json(&[
            ("name".into(), "string".into()),
            ("age".into(), "int".into()),
            ("active".into(), "bool".into()),
            ("tags".into(), "List<string>".into()),
        ]);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["name"], json!("string"));
        assert_eq!(v["age"], json!(0));
        assert_eq!(v["active"], json!(false));
        assert_eq!(v["tags"], json!([]));
    }

    #[test]
    fn resolve_prefers_most_specific_route() {
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = dir.path();
        // Wildcard listed first; the literal /pets/special must still win.
        let reqs = vec![
            (
                wire_dir.join("requests/pets/by-id.wire.yaml"),
                req("GET", "{{baseUrl}}/pets/{{id}}", &[("from", "string")]),
            ),
            (
                wire_dir.join("requests/pets/special.wire.yaml"),
                req("GET", "{{baseUrl}}/pets/special", &[("special", "string")]),
            ),
        ];
        let r = resolve(&reqs, wire_dir, "GET", "/pets/special").unwrap();
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["special"], json!("string")); // literal route, not the {id} one
                                                   // A non-literal id still falls to the wildcard.
        let r2 = resolve(&reqs, wire_dir, "GET", "/pets/42").unwrap();
        let v2: Value = serde_json::from_str(&r2.body).unwrap();
        assert_eq!(v2["from"], json!("string"));
    }

    #[test]
    fn resolve_falls_back_to_schema_then_empty() {
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = dir.path();
        let reqs = vec![
            (
                wire_dir.join("requests/pets/list.wire.yaml"),
                req("GET", "{{baseUrl}}/pets", &[("name", "string")]),
            ),
            (
                wire_dir.join("requests/pets/create.wire.yaml"),
                req("POST", "{{baseUrl}}/pets", &[]),
            ),
        ];

        // Schema-shaped GET
        let r = resolve(&reqs, wire_dir, "GET", "/pets").unwrap();
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["name"], json!("string"));

        // POST with no schema/snapshot -> 201 {}
        let r2 = resolve(&reqs, wire_dir, "POST", "/pets").unwrap();
        assert_eq!(r2.status, 201);
        assert_eq!(r2.body, "{}");

        // No matching route
        assert!(resolve(&reqs, wire_dir, "GET", "/owners").is_none());
        // Wrong method
        assert!(resolve(&reqs, wire_dir, "DELETE", "/pets").is_none());
    }

    #[test]
    fn resolve_prefers_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = dir.path();
        let snap = Snapshot {
            status: 200,
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".into(), "application/json".into());
                h
            },
            body: json!({"id": 1, "name": "Rex"}),
        };
        save_snapshot(&snap, wire_dir, "requests/pets/get.wire.yaml").unwrap();

        let reqs: Vec<(PathBuf, WireRequest)> = vec![(
            wire_dir.join("requests/pets/get.wire.yaml"),
            req("GET", "{{baseUrl}}/pets/{{id}}", &[("ignored", "string")]),
        )];

        let r = resolve(&reqs, wire_dir, "GET", "/pets/42").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/json");
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["name"], json!("Rex")); // from the snapshot, not the schema
    }
}
