use crate::collection::{load_collection, Body, WireRequest};
use crate::error::WireError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Severity of a contract change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Breaking,
    Warning,
    Info,
}

/// A single detected contract change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChange {
    pub severity: Severity,
    pub method: String,
    pub route: String,
    pub description: String,
}

/// Full breaking change report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingReport {
    pub changes: Vec<ContractChange>,
    pub breaking_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

impl BreakingReport {
    pub fn has_breaking_changes(&self) -> bool {
        self.breaking_count > 0
    }
}

/// A snapshot of a single endpoint's contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EndpointSnapshot {
    /// Relative file path within .wire/ directory
    pub file: String,
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// URL pattern (with template variables)
    pub url: String,
    /// Normalized route for comparison (stripped of base URL)
    pub route: String,
    /// Query parameter names
    pub params: BTreeSet<String>,
    /// Header names
    pub headers: BTreeSet<String>,
    /// Body type if present (json, text, formdata)
    pub body_type: Option<String>,
    /// Body field names (for JSON bodies)
    pub body_fields: BTreeSet<String>,
    /// Response schema: field name -> type hint
    pub response_schema: BTreeMap<String, String>,
}

/// Full contract snapshot of a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSnapshot {
    pub version: u32,
    pub created: String,
    pub endpoints: Vec<EndpointSnapshot>,
}

/// Snapshot file name within .wire/ directory.
const SNAPSHOT_FILE: &str = "contract-snapshot.json";

/// Create a contract snapshot from the current collection state.
pub fn create_snapshot(wire_dir: &Path) -> Result<ContractSnapshot, WireError> {
    let collection = load_collection(wire_dir)?;
    let mut endpoints = Vec::new();

    for (path, request) in &collection.requests {
        let file = path
            .strip_prefix(wire_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        endpoints.push(endpoint_from_request(&file, request));
    }

    // Sort for deterministic output
    endpoints.sort_by(|a, b| (&a.method, &a.route).cmp(&(&b.method, &b.route)));

    Ok(ContractSnapshot {
        version: 1,
        created: chrono_now(),
        endpoints,
    })
}

/// Save a contract snapshot to .wire/contract-snapshot.json.
pub fn save_snapshot(wire_dir: &Path) -> Result<(ContractSnapshot, std::path::PathBuf), WireError> {
    let snapshot = create_snapshot(wire_dir)?;
    let path = wire_dir.join(SNAPSHOT_FILE);
    let json = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(&path, json)?;
    Ok((snapshot, path))
}

/// Load a previously saved contract snapshot.
pub fn load_snapshot(wire_dir: &Path) -> Result<ContractSnapshot, WireError> {
    let path = wire_dir.join(SNAPSHOT_FILE);
    if !path.exists() {
        return Err(WireError::Other(format!(
            "No contract snapshot found. Run `wire breaking --save` first to create a baseline.\n  \
             Expected: {}",
            path.display()
        )));
    }
    let content = std::fs::read_to_string(&path)?;
    let snapshot: ContractSnapshot = serde_json::from_str(&content)?;
    Ok(snapshot)
}

/// Compare current collection state against a saved snapshot.
pub fn compare(wire_dir: &Path) -> Result<BreakingReport, WireError> {
    let old = load_snapshot(wire_dir)?;
    let current = create_snapshot(wire_dir)?;
    Ok(diff_snapshots(&old, &current))
}

/// Diff two snapshots and produce a breaking report.
pub fn diff_snapshots(old: &ContractSnapshot, new: &ContractSnapshot) -> BreakingReport {
    let mut changes = Vec::new();

    // Index endpoints by (method, route) for lookup
    let old_map: BTreeMap<(String, String), &EndpointSnapshot> = old
        .endpoints
        .iter()
        .map(|e| ((e.method.clone(), e.route.clone()), e))
        .collect();
    let new_map: BTreeMap<(String, String), &EndpointSnapshot> = new
        .endpoints
        .iter()
        .map(|e| ((e.method.clone(), e.route.clone()), e))
        .collect();

    // Check for removed endpoints (BREAKING)
    for (key, old_ep) in &old_map {
        if !new_map.contains_key(key) {
            changes.push(ContractChange {
                severity: Severity::Breaking,
                method: old_ep.method.clone(),
                route: old_ep.route.clone(),
                description: "endpoint removed".to_string(),
            });
        }
    }

    // Check for added endpoints (INFO)
    for (key, new_ep) in &new_map {
        if !old_map.contains_key(key) {
            changes.push(ContractChange {
                severity: Severity::Info,
                method: new_ep.method.clone(),
                route: new_ep.route.clone(),
                description: "new endpoint added".to_string(),
            });
        }
    }

    // Compare matched endpoints
    for (key, old_ep) in &old_map {
        if let Some(new_ep) = new_map.get(key) {
            compare_endpoints(old_ep, new_ep, &mut changes);
        }
    }

    // Sort by severity (breaking first), then method+route
    changes.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.method.cmp(&b.method))
            .then(a.route.cmp(&b.route))
    });

    let breaking_count = changes
        .iter()
        .filter(|c| c.severity == Severity::Breaking)
        .count();
    let warning_count = changes
        .iter()
        .filter(|c| c.severity == Severity::Warning)
        .count();
    let info_count = changes
        .iter()
        .filter(|c| c.severity == Severity::Info)
        .count();

    BreakingReport {
        changes,
        breaking_count,
        warning_count,
        info_count,
    }
}

/// Compare two endpoint snapshots and append changes.
fn compare_endpoints(
    old: &EndpointSnapshot,
    new: &EndpointSnapshot,
    changes: &mut Vec<ContractChange>,
) {
    let method = &old.method;
    let route = &old.route;

    // Response schema: removed fields are BREAKING
    for (field, old_type) in &old.response_schema {
        match new.response_schema.get(field) {
            None => {
                changes.push(ContractChange {
                    severity: Severity::Breaking,
                    method: method.clone(),
                    route: route.clone(),
                    description: format!("response field '{field}' removed"),
                });
            }
            Some(new_type)
                if new_type != old_type && !old_type.is_empty() && !new_type.is_empty() =>
            {
                changes.push(ContractChange {
                    severity: Severity::Breaking,
                    method: method.clone(),
                    route: route.clone(),
                    description: format!(
                        "response field '{field}' type changed: {old_type} → {new_type}"
                    ),
                });
            }
            _ => {}
        }
    }

    // Response schema: added fields are INFO
    for field in new.response_schema.keys() {
        if !old.response_schema.contains_key(field) {
            changes.push(ContractChange {
                severity: Severity::Info,
                method: method.clone(),
                route: route.clone(),
                description: format!("new response field '{field}'"),
            });
        }
    }

    // Body fields: removed fields are BREAKING
    for field in &old.body_fields {
        if !new.body_fields.contains(field) {
            changes.push(ContractChange {
                severity: Severity::Breaking,
                method: method.clone(),
                route: route.clone(),
                description: format!("body field '{field}' removed"),
            });
        }
    }

    // Body fields: added fields are INFO
    for field in &new.body_fields {
        if !old.body_fields.contains(field) {
            changes.push(ContractChange {
                severity: Severity::Info,
                method: method.clone(),
                route: route.clone(),
                description: format!("new body field '{field}'"),
            });
        }
    }

    // Body type changes
    match (&old.body_type, &new.body_type) {
        (Some(old_bt), Some(new_bt)) if old_bt != new_bt => {
            changes.push(ContractChange {
                severity: Severity::Breaking,
                method: method.clone(),
                route: route.clone(),
                description: format!("body type changed: {old_bt} → {new_bt}"),
            });
        }
        (Some(_), None) => {
            changes.push(ContractChange {
                severity: Severity::Breaking,
                method: method.clone(),
                route: route.clone(),
                description: "request body removed".to_string(),
            });
        }
        (None, Some(new_bt)) => {
            changes.push(ContractChange {
                severity: Severity::Warning,
                method: method.clone(),
                route: route.clone(),
                description: format!("request body added (type: {new_bt})"),
            });
        }
        _ => {}
    }

    // Params: new required params are WARNING, removed are INFO
    for param in new.params.difference(&old.params) {
        changes.push(ContractChange {
            severity: Severity::Warning,
            method: method.clone(),
            route: route.clone(),
            description: format!("new required param '{param}'"),
        });
    }
    for param in old.params.difference(&new.params) {
        changes.push(ContractChange {
            severity: Severity::Info,
            method: method.clone(),
            route: route.clone(),
            description: format!("param '{param}' removed"),
        });
    }

    // Headers: new required headers are WARNING (consumers must now send them),
    // removed headers are INFO (consumers no longer need to send them)
    for header in new.headers.difference(&old.headers) {
        changes.push(ContractChange {
            severity: Severity::Warning,
            method: method.clone(),
            route: route.clone(),
            description: format!("new required header '{header}'"),
        });
    }
    for header in old.headers.difference(&new.headers) {
        changes.push(ContractChange {
            severity: Severity::Info,
            method: method.clone(),
            route: route.clone(),
            description: format!("header '{header}' no longer required"),
        });
    }
}

/// Convert a WireRequest into an EndpointSnapshot.
fn endpoint_from_request(file: &str, request: &WireRequest) -> EndpointSnapshot {
    let route = normalize_route(&request.url);
    let params: BTreeSet<String> = request.params.keys().cloned().collect();
    let headers: BTreeSet<String> = request.headers.keys().cloned().collect();
    let response_schema: BTreeMap<String, String> = request
        .response_schema
        .iter()
        .map(|(name, hint)| (name.clone(), hint.clone()))
        .collect();

    let (body_type, body_fields) = extract_body_info(&request.body);

    EndpointSnapshot {
        file: file.to_string(),
        method: request.method.to_uppercase(),
        url: request.url.clone(),
        route,
        params,
        headers,
        body_type,
        body_fields,
        response_schema,
    }
}

/// Extract body type and field names from a Body.
fn extract_body_info(body: &Option<Body>) -> (Option<String>, BTreeSet<String>) {
    match body {
        None => (None, BTreeSet::new()),
        Some(body) => {
            let body_type = match body.body_type {
                crate::collection::BodyType::Json => "json",
                crate::collection::BodyType::Text => "text",
                crate::collection::BodyType::FormData => "formdata",
            }
            .to_string();

            let mut fields = BTreeSet::new();
            if let Some(obj) = body.content.as_object() {
                for key in obj.keys() {
                    fields.insert(key.clone());
                }
            }

            (Some(body_type), fields)
        }
    }
}

/// Normalize a URL for comparison: strip base URL templates, lowercase, trim trailing slash.
fn normalize_route(url: &str) -> String {
    let mut route = url.to_string();

    // Strip common URL template prefixes
    for prefix in &[
        "{{schema}}://{{baseUrl}}",
        "{{schema}}://{{base_url}}",
        "{{baseUrl}}",
        "{{base_url}}",
    ] {
        if let Some(rest) = route.strip_prefix(prefix) {
            route = rest.to_string();
            break;
        }
    }

    // Strip protocol+host if present
    if route.starts_with("https://") {
        if let Some(pos) = route[8..].find('/') {
            route = route[8 + pos..].to_string();
        }
    } else if route.starts_with("http://") {
        if let Some(pos) = route[7..].find('/') {
            route = route[7 + pos..].to_string();
        }
    }

    route = route.trim_end_matches('/').to_lowercase();

    if !route.starts_with('/') {
        route = format!("/{route}");
    }

    route
}

/// Get current timestamp as ISO 8601 string.
fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Body, BodyType, WireRequest};

    fn make_request(
        name: &str,
        method: &str,
        url: &str,
        params: Vec<&str>,
        headers: Vec<&str>,
        response_schema: Vec<(&str, &str)>,
    ) -> WireRequest {
        WireRequest {
            name: name.to_string(),
            method: method.to_string(),
            url: url.to_string(),
            params: params
                .into_iter()
                .map(|p| (p.to_string(), String::new()))
                .collect(),
            headers: headers
                .into_iter()
                .map(|h| (h.to_string(), String::new()))
                .collect(),
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: response_schema
                .into_iter()
                .map(|(n, t)| (n.to_string(), t.to_string()))
                .collect(),
            chain: Vec::new(),
            snapshot: None,
        }
    }

    fn snapshot_from_requests(requests: Vec<(&str, WireRequest)>) -> ContractSnapshot {
        let endpoints: Vec<EndpointSnapshot> = requests
            .into_iter()
            .map(|(file, req)| endpoint_from_request(file, &req))
            .collect();
        ContractSnapshot {
            version: 1,
            created: "test".to_string(),
            endpoints,
        }
    }

    #[test]
    fn no_changes_produces_empty_report() {
        let req = make_request("Get Users", "GET", "/api/users", vec![], vec![], vec![]);
        let old = snapshot_from_requests(vec![("users.wire.yaml", req.clone())]);
        let new = snapshot_from_requests(vec![("users.wire.yaml", req)]);
        let report = diff_snapshots(&old, &new);
        assert!(report.changes.is_empty());
        assert!(!report.has_breaking_changes());
    }

    #[test]
    fn removed_endpoint_is_breaking() {
        let req = make_request("Get Users", "GET", "/api/users", vec![], vec![], vec![]);
        let old = snapshot_from_requests(vec![("users.wire.yaml", req)]);
        let new = snapshot_from_requests(vec![]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.breaking_count, 1);
        assert!(report.has_breaking_changes());
        assert_eq!(report.changes[0].description, "endpoint removed");
    }

    #[test]
    fn added_endpoint_is_info() {
        let req = make_request("Get Users", "GET", "/api/users", vec![], vec![], vec![]);
        let old = snapshot_from_requests(vec![]);
        let new = snapshot_from_requests(vec![("users.wire.yaml", req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.info_count, 1);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(report.changes[0].description, "new endpoint added");
    }

    #[test]
    fn removed_response_field_is_breaking() {
        let old_req = make_request(
            "Get Users",
            "GET",
            "/api/users",
            vec![],
            vec![],
            vec![("id", "number"), ("email", "string")],
        );
        let new_req = make_request(
            "Get Users",
            "GET",
            "/api/users",
            vec![],
            vec![],
            vec![("id", "number")],
        );
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.breaking_count, 1);
        assert_eq!(
            report.changes[0].description,
            "response field 'email' removed"
        );
    }

    #[test]
    fn response_field_type_change_is_breaking() {
        let old_req = make_request(
            "Get Items",
            "GET",
            "/items",
            vec![],
            vec![],
            vec![("price", "number")],
        );
        let new_req = make_request(
            "Get Items",
            "GET",
            "/items",
            vec![],
            vec![],
            vec![("price", "string")],
        );
        let old = snapshot_from_requests(vec![("i.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("i.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.breaking_count, 1);
        assert_eq!(
            report.changes[0].description,
            "response field 'price' type changed: number → string"
        );
    }

    #[test]
    fn added_response_field_is_info() {
        let old_req = make_request(
            "Get Users",
            "GET",
            "/api/users",
            vec![],
            vec![],
            vec![("id", "number")],
        );
        let new_req = make_request(
            "Get Users",
            "GET",
            "/api/users",
            vec![],
            vec![],
            vec![("id", "number"), ("avatar", "string")],
        );
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.info_count, 1);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(report.changes[0].description, "new response field 'avatar'");
    }

    #[test]
    fn new_required_param_is_warning() {
        let old_req = make_request("Get Items", "GET", "/items", vec![], vec![], vec![]);
        let new_req = make_request(
            "Get Items",
            "GET",
            "/items",
            vec!["tenant_id"],
            vec![],
            vec![],
        );
        let old = snapshot_from_requests(vec![("i.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("i.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(
            report.changes[0].description,
            "new required param 'tenant_id'"
        );
    }

    #[test]
    fn removed_header_is_info() {
        let old_req = make_request(
            "Get Users",
            "GET",
            "/users",
            vec![],
            vec!["Authorization"],
            vec![],
        );
        let new_req = make_request("Get Users", "GET", "/users", vec![], vec![], vec![]);
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.info_count, 1);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(
            report.changes[0].description,
            "header 'Authorization' no longer required"
        );
    }

    #[test]
    fn new_header_is_warning() {
        let old_req = make_request("Get Users", "GET", "/users", vec![], vec![], vec![]);
        let new_req = make_request(
            "Get Users",
            "GET",
            "/users",
            vec![],
            vec!["X-Tenant-Id"],
            vec![],
        );
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.warning_count, 1);
        assert_eq!(
            report.changes[0].description,
            "new required header 'X-Tenant-Id'"
        );
    }

    #[test]
    fn body_field_removed_is_breaking() {
        let mut old_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        old_req.body = Some(Body {
            body_type: BodyType::Json,
            content: serde_json::json!({"name": "", "email": ""}),
        });
        let mut new_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        new_req.body = Some(Body {
            body_type: BodyType::Json,
            content: serde_json::json!({"name": ""}),
        });
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.breaking_count, 1);
        assert_eq!(report.changes[0].description, "body field 'email' removed");
    }

    #[test]
    fn body_type_change_is_breaking() {
        let mut old_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        old_req.body = Some(Body {
            body_type: BodyType::Json,
            content: serde_json::json!({"name": ""}),
        });
        let mut new_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        new_req.body = Some(Body {
            body_type: BodyType::FormData,
            content: serde_json::json!({"name": ""}),
        });
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.breaking_count, 1);
        assert_eq!(
            report.changes[0].description,
            "body type changed: json → formdata"
        );
    }

    #[test]
    fn multiple_changes_across_endpoints() {
        let old_users = make_request(
            "Get Users",
            "GET",
            "/users",
            vec![],
            vec![],
            vec![("id", "number"), ("email", "string")],
        );
        let old_orders = make_request("Create Order", "POST", "/orders", vec![], vec![], vec![]);

        let new_users = make_request(
            "Get Users",
            "GET",
            "/users",
            vec!["tenant"],
            vec![],
            vec![("id", "number"), ("avatar", "string")],
        );
        // orders endpoint removed, items endpoint added
        let new_items = make_request("Get Items", "GET", "/items", vec![], vec![], vec![]);

        let old = snapshot_from_requests(vec![
            ("users.wire.yaml", old_users),
            ("orders.wire.yaml", old_orders),
        ]);
        let new = snapshot_from_requests(vec![
            ("users.wire.yaml", new_users),
            ("items.wire.yaml", new_items),
        ]);
        let report = diff_snapshots(&old, &new);

        // BREAKING: email removed, orders endpoint removed
        assert_eq!(report.breaking_count, 2);
        // WARNING: tenant param added
        assert_eq!(report.warning_count, 1);
        // INFO: avatar field added, items endpoint added
        assert_eq!(report.info_count, 2);
    }

    #[test]
    fn normalize_route_strips_base_url() {
        assert_eq!(
            normalize_route("{{schema}}://{{baseUrl}}/api/users"),
            "/api/users"
        );
        assert_eq!(
            normalize_route("{{schema}}://{{base_url}}/items/{{id}}"),
            "/items/{{id}}"
        );
        assert_eq!(normalize_route("/api/users/"), "/api/users");
        assert_eq!(
            normalize_route("https://example.com/api/items"),
            "/api/items"
        );
        // http:// (7 chars) vs https:// (8 chars)
        assert_eq!(
            normalize_route("http://localhost:3000/api/users"),
            "/api/users"
        );
        assert_eq!(normalize_route("http://x/path"), "/path");
    }

    #[test]
    fn body_removed_is_breaking() {
        let mut old_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        old_req.body = Some(Body {
            body_type: BodyType::Json,
            content: serde_json::json!({"name": ""}),
        });
        let new_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        // 2 breaking: body removed + body field 'name' removed
        assert_eq!(report.breaking_count, 2);
        let descriptions: Vec<&str> = report
            .changes
            .iter()
            .map(|c| c.description.as_str())
            .collect();
        assert!(descriptions.contains(&"request body removed"));
        assert!(descriptions.contains(&"body field 'name' removed"));
    }

    #[test]
    fn body_added_is_warning() {
        let old_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        let mut new_req = make_request("Create User", "POST", "/users", vec![], vec![], vec![]);
        new_req.body = Some(Body {
            body_type: BodyType::Json,
            content: serde_json::json!({"name": ""}),
        });
        let old = snapshot_from_requests(vec![("u.wire.yaml", old_req)]);
        let new = snapshot_from_requests(vec![("u.wire.yaml", new_req)]);
        let report = diff_snapshots(&old, &new);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.breaking_count, 0);
        assert_eq!(
            report.changes[0].description,
            "request body added (type: json)"
        );
    }

    #[test]
    fn snapshot_round_trip() {
        let req = make_request(
            "Get Users",
            "GET",
            "/api/users",
            vec!["page"],
            vec!["Authorization"],
            vec![("id", "number"), ("email", "string")],
        );
        let snapshot = snapshot_from_requests(vec![("users.wire.yaml", req)]);
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let parsed: ContractSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot.endpoints.len(), parsed.endpoints.len());
        assert_eq!(snapshot.endpoints[0], parsed.endpoints[0]);
    }

    #[test]
    fn save_and_load_snapshot_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        std::fs::create_dir_all(wire_dir.join("requests")).unwrap();
        std::fs::create_dir_all(wire_dir.join("envs")).unwrap();

        // Write a minimal collection
        std::fs::write(wire_dir.join("wire.yaml"), "name: Test\nversion: 1\n").unwrap();
        std::fs::write(
            wire_dir.join("requests/get-users.wire.yaml"),
            "name: Get Users\nmethod: GET\nurl: /api/users\n",
        )
        .unwrap();

        // Save snapshot
        let (saved, path) = save_snapshot(&wire_dir).unwrap();
        assert!(path.exists());
        assert_eq!(saved.endpoints.len(), 1);

        // Load snapshot
        let loaded = load_snapshot(&wire_dir).unwrap();
        assert_eq!(loaded.endpoints.len(), 1);
        assert_eq!(loaded.endpoints[0].method, "GET");
        assert_eq!(loaded.endpoints[0].route, "/api/users");
    }

    #[test]
    fn load_snapshot_missing_file_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = load_snapshot(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No contract snapshot found"));
    }
}
