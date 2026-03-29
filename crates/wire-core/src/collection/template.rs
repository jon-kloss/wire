use crate::collection::request::{Body, WireRequest};
use crate::error::WireError;
use std::path::Path;

const MAX_TEMPLATE_DEPTH: usize = 3;

/// Load a template by name from .wire/templates/<name>.wire.yaml
pub fn load_template(name: &str, wire_dir: &Path) -> Result<WireRequest, WireError> {
    // Guard against path traversal (e.g. "../../secrets")
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(WireError::Other(format!(
            "Invalid template name: {name} (must not contain path separators or '..')"
        )));
    }

    let template_path = wire_dir.join("templates").join(format!("{name}.wire.yaml"));
    if !template_path.exists() {
        return Err(WireError::Other(format!(
            "Template not found: {name} (expected at {})",
            template_path.display()
        )));
    }
    let content = std::fs::read_to_string(&template_path)?;
    let template: WireRequest = serde_yaml::from_str(&content)?;
    Ok(template)
}

/// Resolve a request's template chain and return the fully merged request.
/// If the request has no `extends`, returns it unchanged.
pub fn resolve_template(request: WireRequest, wire_dir: &Path) -> Result<WireRequest, WireError> {
    resolve_template_inner(request, wire_dir, &mut Vec::new(), 0)
}

fn resolve_template_inner(
    request: WireRequest,
    wire_dir: &Path,
    chain: &mut Vec<String>,
    depth: usize,
) -> Result<WireRequest, WireError> {
    let template_name = match &request.extends {
        Some(name) => name.clone(),
        None => return Ok(request),
    };

    // Check circular first (before push, so we detect revisiting)
    if chain.contains(&template_name) {
        chain.push(template_name);
        return Err(WireError::Other(format!(
            "Circular template dependency: {}",
            chain.join(" -> ")
        )));
    }

    // Push before depth check so the error message includes the triggering template
    chain.push(template_name.clone());

    if depth >= MAX_TEMPLATE_DEPTH {
        return Err(WireError::Other(format!(
            "Template inheritance too deep (max {MAX_TEMPLATE_DEPTH}): {}",
            chain.join(" -> ")
        )));
    }
    let template = load_template(&template_name, wire_dir)?;
    let resolved_template = resolve_template_inner(template, wire_dir, chain, depth + 1)?;

    let mut merged = merge_requests(&resolved_template, &request);
    // Preserve the original extends for informational purposes (GUI badge)
    merged.extends = Some(template_name);
    Ok(merged)
}

/// Merge a base template with an override request.
/// - URL/method/name: override wins if non-empty
/// - Headers: additive (template + override; override wins on key conflict)
/// - Params: additive (same as headers)
/// - Body: top-level merge when both are JSON objects of same type; override wins otherwise
/// - Tests: override wins entirely if non-empty
/// - extends: cleared (caller may re-set for informational purposes)
fn merge_requests(base: &WireRequest, over: &WireRequest) -> WireRequest {
    // Headers: start with base, override with request
    let mut headers = base.headers.clone();
    for (k, v) in &over.headers {
        headers.insert(k.clone(), v.clone());
    }

    // Params: same additive merge
    let mut params = base.params.clone();
    for (k, v) in &over.params {
        params.insert(k.clone(), v.clone());
    }

    // Body: override wins if present; JSON object top-level merge
    let body = merge_body(&base.body, &over.body);

    // Tests: override wins if non-empty
    let tests = if over.tests.is_empty() {
        base.tests.clone()
    } else {
        over.tests.clone()
    };

    let response_schema = if over.response_schema.is_empty() {
        base.response_schema.clone()
    } else {
        over.response_schema.clone()
    };

    WireRequest {
        name: over.name.clone(),
        method: if over.method.is_empty() {
            base.method.clone()
        } else {
            over.method.clone()
        },
        url: if over.url.is_empty() {
            base.url.clone()
        } else {
            over.url.clone()
        },
        headers,
        params,
        body,
        extends: None, // resolved
        tests,
        response_schema,
    }
}

/// Merge body fields. Override wins if present.
/// If both are JSON objects, do a top-level field merge.
fn merge_body(base: &Option<Body>, over: &Option<Body>) -> Option<Body> {
    match (base, over) {
        (_, Some(over_body)) => {
            if let Some(base_body) = base {
                // Both exist and both are JSON objects: top-level merge
                if base_body.body_type == over_body.body_type {
                    if let (Some(base_obj), Some(over_obj)) =
                        (base_body.content.as_object(), over_body.content.as_object())
                    {
                        let mut merged = base_obj.clone();
                        for (k, v) in over_obj {
                            merged.insert(k.clone(), v.clone());
                        }
                        return Some(Body {
                            body_type: over_body.body_type.clone(),
                            content: serde_json::Value::Object(merged),
                        });
                    }
                }
            }
            // Override wins entirely
            Some(over_body.clone())
        }
        (Some(base_body), None) => Some(base_body.clone()),
        (None, None) => None,
    }
}

/// List all template names available in .wire/templates/
pub fn list_templates(wire_dir: &Path) -> Result<Vec<String>, WireError> {
    let templates_dir = wire_dir.join("templates");
    if !templates_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(&templates_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
            if fname.ends_with(".wire.yaml") {
                names.push(fname.trim_end_matches(".wire.yaml").to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn setup_wire_dir(dir: &Path) -> std::path::PathBuf {
        let wire_dir = dir.join(".wire");
        fs::create_dir_all(wire_dir.join("templates")).unwrap();
        fs::create_dir_all(wire_dir.join("requests")).unwrap();
        wire_dir
    }

    // --- load_template ---

    #[test]
    fn load_template_basic() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/base-api.wire.yaml"),
            "name: Base API\nmethod: GET\nurl: \"{{baseUrl}}\"\nheaders:\n  Accept: application/json\n  Authorization: \"Bearer {{token}}\"\n",
        ).unwrap();

        let template = load_template("base-api", &wire_dir).unwrap();
        assert_eq!(template.name, "Base API");
        assert_eq!(template.headers["Accept"], "application/json");
        assert_eq!(template.headers["Authorization"], "Bearer {{token}}");
    }

    #[test]
    fn load_template_not_found() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        let result = load_template("nonexistent", &wire_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Template not found: nonexistent"));
    }

    // --- merge_requests ---

    #[test]
    fn merge_headers_additive() {
        let base = WireRequest {
            name: "Base".into(),
            method: "GET".into(),
            url: "https://api.example.com".into(),
            headers: HashMap::from([
                ("Accept".into(), "application/json".into()),
                ("X-Api-Key".into(), "{{key}}".into()),
            ]),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "https://api.example.com/users".into(),
            headers: HashMap::from([("X-Request-Id".into(), "abc123".into())]),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        assert_eq!(merged.name, "Override");
        assert_eq!(merged.method, "GET"); // from base (override is empty)
        assert_eq!(merged.url, "https://api.example.com/users"); // from override
        assert_eq!(merged.headers.len(), 3); // all three headers
        assert_eq!(merged.headers["Accept"], "application/json");
        assert_eq!(merged.headers["X-Api-Key"], "{{key}}");
        assert_eq!(merged.headers["X-Request-Id"], "abc123");
    }

    #[test]
    fn merge_header_override_same_key() {
        let base = WireRequest {
            name: "Base".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::from([("Authorization".into(), "Bearer {{base_token}}".into())]),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::from([("Authorization".into(), "Basic abc".into())]),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        assert_eq!(merged.headers["Authorization"], "Basic abc");
    }

    #[test]
    fn merge_params_additive() {
        let base = WireRequest {
            name: "Base".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::from([("page".into(), "1".into())]),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::from([("limit".into(), "10".into())]),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        assert_eq!(merged.params.len(), 2);
        assert_eq!(merged.params["page"], "1");
        assert_eq!(merged.params["limit"], "10");
    }

    #[test]
    fn merge_body_override_wins() {
        use crate::collection::request::BodyType;

        let base = WireRequest {
            name: "Base".into(),
            method: "POST".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"base_field": "base_value"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"over_field": "over_value"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        let body = merged.body.unwrap();
        // Top-level JSON merge: both fields present
        assert_eq!(body.content["base_field"], "base_value");
        assert_eq!(body.content["over_field"], "over_value");
    }

    #[test]
    fn merge_body_json_override_wins_on_conflict() {
        use crate::collection::request::BodyType;

        let base = WireRequest {
            name: "Base".into(),
            method: "POST".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"name": "old", "age": 30}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"name": "new"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        let body = merged.body.unwrap();
        assert_eq!(body.content["name"], "new"); // override wins
        assert_eq!(body.content["age"], 30); // base preserved
    }

    #[test]
    fn merge_body_base_only() {
        use crate::collection::request::BodyType;

        let base = WireRequest {
            name: "Base".into(),
            method: "POST".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"field": "value"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        assert!(merged.body.is_some());
        assert_eq!(merged.body.unwrap().content["field"], "value");
    }

    // --- resolve_template (integration) ---

    #[test]
    fn resolve_simple_extends() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/authenticated.wire.yaml"),
            "name: Authenticated\nmethod: GET\nurl: \"{{baseUrl}}\"\nheaders:\n  Authorization: \"Bearer {{token}}\"\n  Accept: application/json\n",
        ).unwrap();

        let request = WireRequest {
            name: "Get Users".into(),
            method: "GET".into(),
            url: "{{baseUrl}}/users".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("authenticated".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let resolved = resolve_template(request, &wire_dir).unwrap();
        assert_eq!(resolved.name, "Get Users");
        assert_eq!(resolved.url, "{{baseUrl}}/users");
        assert_eq!(resolved.headers["Authorization"], "Bearer {{token}}");
        assert_eq!(resolved.headers["Accept"], "application/json");
        assert_eq!(resolved.extends, Some("authenticated".into())); // preserved for GUI
    }

    #[test]
    fn resolve_chained_templates() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        // base-api -> provides Accept header
        fs::write(
            wire_dir.join("templates/base-api.wire.yaml"),
            "name: Base API\nmethod: GET\nurl: \"{{baseUrl}}\"\nheaders:\n  Accept: application/json\n",
        ).unwrap();

        // authenticated -> extends base-api, adds auth header
        fs::write(
            wire_dir.join("templates/authenticated.wire.yaml"),
            "name: Authenticated\nmethod: GET\nurl: \"\"\nextends: base-api\nheaders:\n  Authorization: \"Bearer {{token}}\"\n",
        ).unwrap();

        let request = WireRequest {
            name: "Get Users".into(),
            method: "GET".into(),
            url: "{{baseUrl}}/users".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("authenticated".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let resolved = resolve_template(request, &wire_dir).unwrap();
        assert_eq!(resolved.headers.len(), 2);
        assert_eq!(resolved.headers["Accept"], "application/json"); // from base-api
        assert_eq!(resolved.headers["Authorization"], "Bearer {{token}}"); // from authenticated
    }

    #[test]
    fn resolve_circular_dependency_detected() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/a.wire.yaml"),
            "name: A\nmethod: GET\nurl: \"\"\nextends: b\n",
        )
        .unwrap();

        fs::write(
            wire_dir.join("templates/b.wire.yaml"),
            "name: B\nmethod: GET\nurl: \"\"\nextends: a\n",
        )
        .unwrap();

        let request = WireRequest {
            name: "Test".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("a".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let result = resolve_template(request, &wire_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Circular template dependency"));
    }

    #[test]
    fn resolve_max_depth_exceeded() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        // Chain: d -> c -> b -> a (depth 3 for the request extending d = 4 total)
        fs::write(
            wire_dir.join("templates/a.wire.yaml"),
            "name: A\nmethod: GET\nurl: \"{{baseUrl}}\"\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/b.wire.yaml"),
            "name: B\nmethod: GET\nurl: \"\"\nextends: a\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/c.wire.yaml"),
            "name: C\nmethod: GET\nurl: \"\"\nextends: b\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/d.wire.yaml"),
            "name: D\nmethod: GET\nurl: \"\"\nextends: c\n",
        )
        .unwrap();

        let request = WireRequest {
            name: "Test".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("d".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let result = resolve_template(request, &wire_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too deep"));
    }

    #[test]
    fn resolve_no_extends_passthrough() {
        let request = WireRequest {
            name: "Simple".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        let resolved = resolve_template(request.clone(), &wire_dir).unwrap();
        assert_eq!(resolved, request);
    }

    // --- list_templates ---

    #[test]
    fn list_templates_empty() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        let names = list_templates(&wire_dir).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn list_templates_finds_all() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/base-api.wire.yaml"),
            "name: Base\nmethod: GET\nurl: x\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/authenticated.wire.yaml"),
            "name: Auth\nmethod: GET\nurl: x\n",
        )
        .unwrap();
        // Non-.wire.yaml file should be ignored
        fs::write(wire_dir.join("templates/notes.txt"), "not a template").unwrap();

        let names = list_templates(&wire_dir).unwrap();
        assert_eq!(names, vec!["authenticated", "base-api"]); // sorted
    }

    #[test]
    fn list_templates_no_templates_dir() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(&wire_dir).unwrap();
        // No templates/ directory

        let names = list_templates(&wire_dir).unwrap();
        assert!(names.is_empty());
    }

    // --- Security: path traversal ---

    #[test]
    fn load_template_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        let result = load_template("../../etc/passwd", &wire_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid template name"));
    }

    #[test]
    fn load_template_rejects_slash_in_name() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        let result = load_template("subdir/template", &wire_dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid template name"));
    }

    // --- Merge edge cases ---

    #[test]
    fn merge_params_override_same_key() {
        let base = WireRequest {
            name: "Base".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::from([("page".into(), "1".into()), ("limit".into(), "10".into())]),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::from([("page".into(), "5".into())]),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        assert_eq!(merged.params["page"], "5"); // override wins
        assert_eq!(merged.params["limit"], "10"); // base preserved
    }

    #[test]
    fn merge_body_different_types_override_wins() {
        use crate::collection::request::BodyType;

        let base = WireRequest {
            name: "Base".into(),
            method: "POST".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"field": "value"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Text,
                content: serde_json::json!("raw text body"),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        let body = merged.body.unwrap();
        assert_eq!(body.body_type, BodyType::Text); // override type wins
        assert_eq!(body.content, serde_json::json!("raw text body"));
    }

    #[test]
    fn merge_body_base_preserves_type() {
        use crate::collection::request::BodyType;

        let base = WireRequest {
            name: "Base".into(),
            method: "POST".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"field": "value"}),
            }),
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let over = WireRequest {
            name: "Override".into(),
            method: "".into(),
            url: "".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: vec![],
            response_schema: vec![],
        };

        let merged = merge_requests(&base, &over);
        let body = merged.body.unwrap();
        assert_eq!(body.body_type, BodyType::Json); // base type preserved
        assert_eq!(body.content["field"], "value");
    }

    #[test]
    fn resolve_exactly_max_depth_succeeds() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        // Chain of exactly 3: c -> b -> a (max depth = 3, so this should succeed)
        fs::write(
            wire_dir.join("templates/a.wire.yaml"),
            "name: A\nmethod: GET\nurl: \"{{baseUrl}}\"\nheaders:\n  X-A: a\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/b.wire.yaml"),
            "name: B\nmethod: GET\nurl: \"\"\nextends: a\nheaders:\n  X-B: b\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/c.wire.yaml"),
            "name: C\nmethod: GET\nurl: \"\"\nextends: b\nheaders:\n  X-C: c\n",
        )
        .unwrap();

        let request = WireRequest {
            name: "Test".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("c".into()),
            tests: vec![],
            response_schema: vec![],
        };

        // Depth: request -> c (depth 0) -> b (depth 1) -> a (depth 2) = 3 levels, should succeed
        let result = resolve_template(request, &wire_dir);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.headers["X-A"], "a");
        assert_eq!(resolved.headers["X-B"], "b");
        assert_eq!(resolved.headers["X-C"], "c");
    }

    #[test]
    fn resolve_circular_self_reference() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/self.wire.yaml"),
            "name: Self\nmethod: GET\nurl: \"\"\nextends: self\n",
        )
        .unwrap();

        let request = WireRequest {
            name: "Test".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("self".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let result = resolve_template(request, &wire_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Circular template dependency"));
        assert!(err.contains("self"));
    }

    #[test]
    fn resolve_circular_includes_chain_in_error() {
        let dir = TempDir::new().unwrap();
        let wire_dir = setup_wire_dir(dir.path());

        fs::write(
            wire_dir.join("templates/a.wire.yaml"),
            "name: A\nmethod: GET\nurl: \"\"\nextends: b\n",
        )
        .unwrap();
        fs::write(
            wire_dir.join("templates/b.wire.yaml"),
            "name: B\nmethod: GET\nurl: \"\"\nextends: a\n",
        )
        .unwrap();

        let request = WireRequest {
            name: "Test".into(),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: Some("a".into()),
            tests: vec![],
            response_schema: vec![],
        };

        let result = resolve_template(request, &wire_dir);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("a -> b -> a")); // chain included in error
    }
}
