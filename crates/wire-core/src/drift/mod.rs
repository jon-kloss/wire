use crate::collection::WireRequest;
use crate::scan::types::DiscoveredEndpoint;
use serde::Serialize;
use std::path::PathBuf;

/// Category of drift between code and collection.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftCategory {
    /// Endpoint exists in code but not in collection.
    New,
    /// Request exists in collection but route not found in code.
    Stale,
    /// Route exists in both but parameters differ.
    Changed,
}

/// A single drift item.
#[derive(Debug, Clone, Serialize)]
pub struct DriftItem {
    pub category: DriftCategory,
    pub method: String,
    pub route: String,
    pub name: String,
    /// For Changed: what specifically changed
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    /// Path to the .wire.yaml file (for Stale/Changed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_path: Option<String>,
}

/// Full drift report.
#[derive(Debug, Clone, Serialize)]
pub struct DriftReport {
    pub items: Vec<DriftItem>,
    pub new_count: usize,
    pub stale_count: usize,
    pub changed_count: usize,
}

impl DriftReport {
    pub fn has_drift(&self) -> bool {
        !self.items.is_empty()
    }
}

/// Normalize a route for comparison.
/// Strips {{schema}}://{{baseUrl}} prefix, lowercases, removes trailing slash,
/// and converts all parameter syntaxes to a canonical form.
fn normalize_route(raw: &str) -> String {
    let mut route = raw.to_string();

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

    // Strip protocol+host if present (e.g., https://example.com/api/...)
    if route.starts_with("http://") || route.starts_with("https://") {
        if let Some(pos) = route[8..].find('/') {
            route = route[8 + pos..].to_string();
        }
    }

    // Strip type constraints like {id:guid} -> {id} (before brace normalization)
    let re_constraint = regex::Regex::new(r"\{(\w+):[^}]*\}").unwrap();
    route = re_constraint.replace_all(&route, "{$1}").to_string();

    // Normalize parameter syntax: {{param}}, :param -> {param}
    let re_double_brace = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();
    route = re_double_brace.replace_all(&route, "{$1}").to_string();

    let re_colon = regex::Regex::new(r":(\w+)").unwrap();
    route = re_colon.replace_all(&route, "{$1}").to_string();

    // Remove trailing slash, lowercase
    route = route.trim_end_matches('/').to_lowercase();

    // Ensure starts with /
    if !route.starts_with('/') {
        route = format!("/{route}");
    }

    route
}

/// Compare scanned endpoints against collection requests to detect drift.
pub fn compare(
    endpoints: &[DiscoveredEndpoint],
    requests: &[(PathBuf, WireRequest)],
) -> DriftReport {
    let mut items = Vec::new();

    // Build a map of normalized (method, route) -> endpoint for quick lookup
    let endpoint_map: std::collections::HashMap<(String, String), &DiscoveredEndpoint> = endpoints
        .iter()
        .map(|ep| {
            let key = (ep.method.to_uppercase(), normalize_route(&ep.route));
            (key, ep)
        })
        .collect();

    // Build a map of normalized (method, route) -> (path, request)
    let request_map: std::collections::HashMap<(String, String), (&PathBuf, &WireRequest)> =
        requests
            .iter()
            .map(|(path, req)| {
                let key = (req.method.to_uppercase(), normalize_route(&req.url));
                (key, (path, req))
            })
            .collect();

    // Find NEW: endpoints in code but not in collection
    for (key, ep) in &endpoint_map {
        if !request_map.contains_key(key) {
            items.push(DriftItem {
                category: DriftCategory::New,
                method: ep.method.clone(),
                route: ep.route.clone(),
                name: ep.name.clone(),
                changes: Vec::new(),
                request_path: None,
            });
        }
    }

    // Find STALE: requests in collection but not in code
    for (key, (path, req)) in &request_map {
        if !endpoint_map.contains_key(key) {
            items.push(DriftItem {
                category: DriftCategory::Stale,
                method: req.method.clone(),
                route: req.url.clone(),
                name: req.name.clone(),
                changes: Vec::new(),
                request_path: Some(path.to_string_lossy().to_string()),
            });
        }
    }

    // Find CHANGED: route exists in both but parameters differ
    for (key, ep) in &endpoint_map {
        if let Some((path, req)) = request_map.get(key) {
            let mut changes = Vec::new();

            // Compare query params
            let ep_params: std::collections::HashSet<&str> =
                ep.query_params.iter().map(|(n, _)| n.as_str()).collect();
            let req_params: std::collections::HashSet<&str> =
                req.params.keys().map(|s| s.as_str()).collect();
            if ep_params != req_params {
                let added: Vec<_> = ep_params.difference(&req_params).collect();
                let removed: Vec<_> = req_params.difference(&ep_params).collect();
                if !added.is_empty() {
                    changes.push(format!(
                        "new query params: {}",
                        added.into_iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
                if !removed.is_empty() {
                    changes.push(format!(
                        "removed query params: {}",
                        removed.into_iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
            }

            // Compare body fields (case-insensitive: scan returns PascalCase,
            // endpoint_to_request converts to camelCase for JSON)
            if !ep.body_fields.is_empty() {
                let ep_fields: std::collections::HashSet<String> = ep
                    .body_fields
                    .iter()
                    .map(|(n, _)| n.to_lowercase())
                    .collect();
                if let Some(ref body) = req.body {
                    if let Some(obj) = body.content.as_object() {
                        let req_fields: std::collections::HashSet<String> =
                            obj.keys().map(|s| s.to_lowercase()).collect();
                        if ep_fields != req_fields {
                            let added: Vec<_> = ep_fields.difference(&req_fields).collect();
                            let removed: Vec<_> = req_fields.difference(&ep_fields).collect();
                            if !added.is_empty() {
                                changes.push(format!(
                                    "new body fields: {}",
                                    added.into_iter().cloned().collect::<Vec<_>>().join(", ")
                                ));
                            }
                            if !removed.is_empty() {
                                changes.push(format!(
                                    "removed body fields: {}",
                                    removed.into_iter().cloned().collect::<Vec<_>>().join(", ")
                                ));
                            }
                        }
                    }
                }
            }

            // Compare response schema (case-insensitive)
            if !ep.response_fields.is_empty() {
                let ep_response: std::collections::HashSet<String> = ep
                    .response_fields
                    .iter()
                    .map(|(n, _)| n.to_lowercase())
                    .collect();
                let req_response: std::collections::HashSet<String> = req
                    .response_schema
                    .iter()
                    .map(|(n, _)| n.to_lowercase())
                    .collect();
                if ep_response != req_response {
                    changes.push("response schema changed".to_string());
                }
            }

            if !changes.is_empty() {
                items.push(DriftItem {
                    category: DriftCategory::Changed,
                    method: ep.method.clone(),
                    route: ep.route.clone(),
                    name: ep.name.clone(),
                    changes,
                    request_path: Some(path.to_string_lossy().to_string()),
                });
            }
        }
    }

    // Sort: New first, then Changed, then Stale
    items.sort_by(|a, b| {
        let order = |c: &DriftCategory| match c {
            DriftCategory::New => 0,
            DriftCategory::Changed => 1,
            DriftCategory::Stale => 2,
        };
        order(&a.category)
            .cmp(&order(&b.category))
            .then(a.method.cmp(&b.method))
            .then(a.route.cmp(&b.route))
    });

    let new_count = items
        .iter()
        .filter(|i| i.category == DriftCategory::New)
        .count();
    let stale_count = items
        .iter()
        .filter(|i| i.category == DriftCategory::Stale)
        .count();
    let changed_count = items
        .iter()
        .filter(|i| i.category == DriftCategory::Changed)
        .count();

    DriftReport {
        items,
        new_count,
        stale_count,
        changed_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_endpoint(method: &str, route: &str) -> DiscoveredEndpoint {
        DiscoveredEndpoint {
            method: method.into(),
            route: route.into(),
            name: format!("{method} {route}"),
            headers: Vec::new(),
            query_params: Vec::new(),
            body_type: None,
            body_fields: Vec::new(),
            response_type: None,
            response_fields: Vec::new(),
        }
    }

    fn make_request(method: &str, url: &str, name: &str) -> (PathBuf, WireRequest) {
        (
            PathBuf::from(format!("requests/{}.wire.yaml", name)),
            WireRequest {
                name: name.into(),
                method: method.into(),
                url: url.into(),
                headers: HashMap::new(),
                params: HashMap::new(),
                body: None,
                extends: None,
                tests: Vec::new(),
                response_schema: Vec::new(),
            },
        )
    }

    // --- normalize_route ---

    #[test]
    fn normalize_strips_base_url_prefix() {
        assert_eq!(
            normalize_route("{{schema}}://{{baseUrl}}/api/users"),
            "/api/users"
        );
    }

    #[test]
    fn normalize_double_brace_params() {
        assert_eq!(normalize_route("/api/users/{{id}}"), "/api/users/{id}");
    }

    #[test]
    fn normalize_colon_params() {
        assert_eq!(normalize_route("/api/users/:id"), "/api/users/{id}");
    }

    #[test]
    fn normalize_type_constraints() {
        assert_eq!(normalize_route("/api/users/{id:guid}"), "/api/users/{id}");
    }

    #[test]
    fn normalize_trailing_slash() {
        assert_eq!(normalize_route("/api/users/"), "/api/users");
    }

    #[test]
    fn normalize_case_insensitive() {
        assert_eq!(normalize_route("/API/Users"), "/api/users");
    }

    #[test]
    fn normalize_full_wire_url() {
        assert_eq!(
            normalize_route("{{schema}}://{{baseUrl}}/api/tours/{{id}}"),
            "/api/tours/{id}"
        );
    }

    // --- compare: NEW ---

    #[test]
    fn detect_new_endpoint() {
        let endpoints = vec![
            make_endpoint("GET", "/api/users"),
            make_endpoint("POST", "/api/users"),
        ];
        let requests = vec![make_request(
            "GET",
            "{{schema}}://{{baseUrl}}/api/users",
            "list-users",
        )];

        let report = compare(&endpoints, &requests);
        assert_eq!(report.new_count, 1);
        assert_eq!(report.items[0].category, DriftCategory::New);
        assert_eq!(report.items[0].method, "POST");
    }

    // --- compare: STALE ---

    #[test]
    fn detect_stale_request() {
        let endpoints = vec![make_endpoint("GET", "/api/users")];
        let requests = vec![
            make_request("GET", "{{schema}}://{{baseUrl}}/api/users", "list-users"),
            make_request(
                "DELETE",
                "{{schema}}://{{baseUrl}}/api/users/{{id}}",
                "delete-user",
            ),
        ];

        let report = compare(&endpoints, &requests);
        assert_eq!(report.stale_count, 1);
        let stale = report
            .items
            .iter()
            .find(|i| i.category == DriftCategory::Stale)
            .unwrap();
        assert_eq!(stale.method, "DELETE");
    }

    // --- compare: CHANGED ---

    #[test]
    fn detect_changed_query_params() {
        let mut ep = make_endpoint("GET", "/api/users");
        ep.query_params = vec![("page".into(), "".into()), ("limit".into(), "".into())];

        let mut req = make_request("GET", "{{schema}}://{{baseUrl}}/api/users", "list-users");
        req.1.params.insert("page".into(), "1".into());
        // Missing "limit" param

        let report = compare(&[ep], &[req]);
        assert_eq!(report.changed_count, 1);
        assert!(report.items[0].changes[0].contains("limit"));
    }

    // --- compare: no drift ---

    #[test]
    fn no_drift_when_matching() {
        let endpoints = vec![
            make_endpoint("GET", "/api/users"),
            make_endpoint("POST", "/api/users"),
        ];
        let requests = vec![
            make_request("GET", "{{schema}}://{{baseUrl}}/api/users", "list"),
            make_request("POST", "{{schema}}://{{baseUrl}}/api/users", "create"),
        ];

        let report = compare(&endpoints, &requests);
        assert!(!report.has_drift());
    }

    #[test]
    fn route_normalization_matches_across_syntaxes() {
        let endpoints = vec![make_endpoint("GET", "/api/users/:id")];
        let requests = vec![make_request(
            "GET",
            "{{schema}}://{{baseUrl}}/api/users/{{id}}",
            "get-user",
        )];

        let report = compare(&endpoints, &requests);
        assert!(!report.has_drift());
    }

    #[test]
    fn empty_inputs() {
        let report = compare(&[], &[]);
        assert!(!report.has_drift());
        assert_eq!(report.new_count, 0);
        assert_eq!(report.stale_count, 0);
        assert_eq!(report.changed_count, 0);
    }
}
