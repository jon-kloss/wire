use super::types::DiscoveredEndpoint;
use regex::Regex;
use std::path::Path;

/// Scan a Node/Express project for HTTP endpoints.
/// Detects router.get/post/etc and app.get/post/etc patterns in .js and .ts files.
pub fn scan_express(project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    let mut endpoints = Vec::new();
    let mut files_scanned = 0;

    let js_files = collect_js_files(project_dir);
    for file_path in &js_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        files_scanned += 1;

        let file_endpoints = parse_express_routes(&content, file_path);
        endpoints.extend(file_endpoints);
    }

    (endpoints, files_scanned)
}

/// Collect all .js and .ts files, skipping irrelevant directories.
fn collect_js_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_js_files_recursive(dir, &mut files, 10);
    files
}

fn collect_js_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>, depth: u32) {
    if depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "node_modules" | ".git" | "dist" | "build" | ".wire" | "target" | "coverage"
            ) {
                continue;
            }
            collect_js_files_recursive(&path, files, depth - 1);
        } else if path
            .extension()
            .is_some_and(|e| e == "js" || e == "ts" || e == "mjs")
        {
            // Skip declaration files and test files
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if fname.ends_with(".d.ts")
                || fname.ends_with(".test.ts")
                || fname.ends_with(".test.js")
                || fname.ends_with(".spec.ts")
                || fname.ends_with(".spec.js")
            {
                continue;
            }
            files.push(path);
        }
    }
}

/// Parse Express route patterns from file content.
///
/// Matches patterns like:
/// - `router.get('/users', ...)`
/// - `app.post('/api/users', ...)`
/// - `router.route('/users').get(...).post(...)`
fn parse_express_routes(content: &str, file_path: &Path) -> Vec<DiscoveredEndpoint> {
    let mut endpoints = Vec::new();

    // Derive group from filename: routes/users.js -> "users"
    let file_group = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("root")
        .to_lowercase();
    // Skip generic filenames — fall back to route prefix for these
    let use_file_group = !matches!(
        file_group.as_str(),
        "index" | "app" | "server" | "main" | "routes"
    );

    // Match: router.get('/route', ...) or app.get('/route', ...)
    // Supports both single and double quotes
    let route_re = Regex::new(
        r#"(?m)(?:app|router|server)\s*\.\s*(get|post|put|patch|delete)\s*\(\s*['"]([^'"]+)['"]\s*,"#,
    )
    .unwrap();

    for cap in route_re.captures_iter(content) {
        let http_method = cap[1].to_uppercase();
        let route = &cap[2];
        let wire_route = convert_express_params(route);
        let name = derive_name(&http_method, route);

        let group = if use_file_group {
            file_group.clone()
        } else {
            group_from_route(route)
        };

        // Try to extract body/query/header usage from the handler
        let handler_start = cap.get(0).unwrap().end();
        let handler_body = extract_handler_body(content, handler_start);
        let ((headers, query_params, body_type), body_fields) = extract_req_usage(&handler_body);

        endpoints.push(DiscoveredEndpoint {
            group,
            method: http_method,
            route: wire_route,
            name,
            headers,
            query_params,
            body_type,
            body_fields,
            response_type: None,
            response_fields: Vec::new(),
        });
    }

    // Match chained route: router.route('/users').get(...).post(...)
    let chained_re =
        Regex::new(r#"(?m)(?:app|router)\s*\.\s*route\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    let chain_method_re = Regex::new(r#"\.\s*(get|post|put|patch|delete)\s*\("#).unwrap();

    for route_cap in chained_re.captures_iter(content) {
        let route = &route_cap[1];
        let wire_route = convert_express_params(route);
        let after_route = &content[route_cap.get(0).unwrap().end()..];

        let group = if use_file_group {
            file_group.clone()
        } else {
            group_from_route(route)
        };

        // Find chained methods following the .route() call
        // Limit scan to ~500 chars to avoid matching unrelated routes
        let scan_limit = after_route.len().min(500);
        let scan_region = &after_route[..scan_limit];

        for method_cap in chain_method_re.captures_iter(scan_region) {
            let http_method = method_cap[1].to_uppercase();
            let name = derive_name(&http_method, route);

            // Stop if we hit a new route() call
            let method_pos = method_cap.get(0).unwrap().start();
            if scan_region[..method_pos].contains(".route(") {
                break;
            }

            endpoints.push(DiscoveredEndpoint {
                group: group.clone(),
                method: http_method,
                route: wire_route.clone(),
                name,
                headers: Vec::new(),
                query_params: Vec::new(),
                body_type: None,
                body_fields: Vec::new(),
                response_type: None,
                response_fields: Vec::new(),
            });
        }
    }

    endpoints
}

/// Derive group from route prefix: /api/users/:id -> "users"
fn group_from_route(route: &str) -> String {
    route
        .trim_start_matches('/')
        .split('/')
        .find(|s| !s.is_empty() && *s != "api" && !s.starts_with(':') && !s.starts_with('{'))
        .unwrap_or("root")
        .to_lowercase()
}

/// Convert Express route parameters :param to Wire {{param}} syntax.
fn convert_express_params(route: &str) -> String {
    let re = Regex::new(r":(\w+)").unwrap();
    re.replace_all(route, "{{$1}}").to_string()
}

/// Derive a human-readable name from a route pattern.
/// e.g., GET /api/users/:id → "GetUsersById"
fn derive_name(method: &str, route: &str) -> String {
    let parts: Vec<&str> = route
        .split('/')
        .filter(|s| !s.is_empty() && *s != "api" && *s != "v1" && *s != "v2")
        .collect();

    let mut name = method
        .chars()
        .next()
        .unwrap_or('G')
        .to_uppercase()
        .to_string()
        + &method[1..].to_lowercase();

    for part in parts {
        if let Some(param) = part.strip_prefix(':') {
            name.push_str("By");
            name.push_str(&capitalize(param));
        } else {
            name.push_str(&capitalize(part));
        }
    }

    if name.len() <= method.len() {
        name.push_str("Root");
    }

    name
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Extract a rough handler body (up to ~500 chars or the next route definition).
fn extract_handler_body(content: &str, start: usize) -> String {
    let remaining = &content[start..];
    // Take up to 500 chars or until we hit another route definition
    let end = remaining
        .find("\nrouter.")
        .or_else(|| remaining.find("\napp."))
        .unwrap_or_else(|| remaining.len().min(500));
    remaining[..end].to_string()
}

/// Extracted parameter metadata: (headers, query_params, body_type)
type ParamMeta = (Vec<(String, String)>, Vec<(String, String)>, Option<String>);

/// Extracted body field metadata
type BodyFields = Vec<(String, String)>;

/// Extract req.body, req.query, req.headers usage from handler body.
/// Returns (headers, query_params, body_type, body_fields)
fn extract_req_usage(handler: &str) -> (ParamMeta, BodyFields) {
    let mut headers = Vec::new();
    let mut query_params = Vec::new();
    let mut body_type = None;
    let mut body_fields: Vec<(String, String)> = Vec::new();

    // Detect req.body usage → indicates body expected
    if handler.contains("req.body") {
        body_type = Some("JSON".to_string());

        // Extract destructured body fields: const { name, email } = req.body
        let destructure_body_re =
            Regex::new(r#"(?:const|let|var)\s+\{\s*([^}]+)\}\s*=\s*req\.body"#).unwrap();
        let mut seen_body = std::collections::HashSet::new();
        for cap in destructure_body_re.captures_iter(handler) {
            for field in cap[1].split(',') {
                let field = field.trim().to_string();
                if !field.is_empty() && seen_body.insert(field.clone()) {
                    body_fields.push((field, String::new()));
                }
            }
        }

        // Also extract req.body.fieldName direct access
        let body_dot_re = Regex::new(r#"req\.body\.(\w+)"#).unwrap();
        for cap in body_dot_re.captures_iter(handler) {
            let field = cap[1].to_string();
            if seen_body.insert(field.clone()) {
                body_fields.push((field, String::new()));
            }
        }
    }

    // Extract req.query.paramName
    let query_re = Regex::new(r#"req\.query\.(\w+)"#).unwrap();
    let mut seen_query = std::collections::HashSet::new();
    for cap in query_re.captures_iter(handler) {
        let param = cap[1].to_string();
        if seen_query.insert(param.clone()) {
            query_params.push((param, String::new()));
        }
    }

    // Also extract destructured: const { param1, param2 } = req.query
    let destructure_query_re =
        Regex::new(r#"(?:const|let|var)\s+\{\s*([^}]+)\}\s*=\s*req\.query"#).unwrap();
    for cap in destructure_query_re.captures_iter(handler) {
        for param in cap[1].split(',') {
            let param = param.trim().to_string();
            if !param.is_empty() && seen_query.insert(param.clone()) {
                query_params.push((param, String::new()));
            }
        }
    }

    // Extract req.headers['header-name'] or req.get('header-name')
    let header_bracket_re = Regex::new(r#"req\.headers\[['"]([^'"]+)['"]\]"#).unwrap();
    let header_get_re = Regex::new(r#"req\.get\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    let mut seen_headers = std::collections::HashSet::new();
    for cap in header_bracket_re.captures_iter(handler) {
        let h = cap[1].to_string();
        if seen_headers.insert(h.clone()) {
            headers.push((h, String::new()));
        }
    }
    for cap in header_get_re.captures_iter(handler) {
        let h = cap[1].to_string();
        if seen_headers.insert(h.clone()) {
            headers.push((h, String::new()));
        }
    }

    ((headers, query_params, body_type), body_fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_basic_express_routes() {
        let code = r#"
const express = require('express');
const router = express.Router();

router.get('/users', (req, res) => {
    res.json([]);
});

router.post('/users', (req, res) => {
    const user = req.body;
    res.json(user);
});

router.get('/users/:id', (req, res) => {
    res.json({});
});

router.delete('/users/:id', (req, res) => {
    res.sendStatus(204);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 4);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[0].name, "GetUsers");

        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/users");
        assert_eq!(endpoints[1].body_type, Some("JSON".to_string()));

        assert_eq!(endpoints[2].method, "GET");
        assert_eq!(endpoints[2].route, "/users/{{id}}");
        assert_eq!(endpoints[2].name, "GetUsersById");

        assert_eq!(endpoints[3].method, "DELETE");
        assert_eq!(endpoints[3].route, "/users/{{id}}");
    }

    #[test]
    fn parse_app_routes() {
        let code = r#"
const app = express();

app.get('/api/health', (req, res) => {
    res.json({ status: 'ok' });
});

app.post('/api/users', (req, res) => {
    res.json(req.body);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 2);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/health");
        assert_eq!(endpoints[0].name, "GetHealth");

        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/api/users");
    }

    #[test]
    fn parse_routes_with_query_params() {
        let code = r#"
router.get('/search', (req, res) => {
    const q = req.query.q;
    const page = req.query.page;
    const limit = req.query.limit;
    res.json([]);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].query_params.len(), 3);
        assert_eq!(endpoints[0].query_params[0].0, "q");
        assert_eq!(endpoints[0].query_params[1].0, "page");
        assert_eq!(endpoints[0].query_params[2].0, "limit");
    }

    #[test]
    fn parse_routes_with_destructured_query() {
        let code = r#"
router.get('/search', (req, res) => {
    const { q, page } = req.query;
    res.json([]);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].query_params.len(), 2);
        assert_eq!(endpoints[0].query_params[0].0, "q");
        assert_eq!(endpoints[0].query_params[1].0, "page");
    }

    #[test]
    fn parse_routes_with_headers() {
        let code = r#"
router.get('/protected', (req, res) => {
    const token = req.headers['authorization'];
    const requestId = req.get('x-request-id');
    res.json({});
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].headers.len(), 2);
        assert_eq!(endpoints[0].headers[0].0, "authorization");
        assert_eq!(endpoints[0].headers[1].0, "x-request-id");
    }

    #[test]
    fn convert_express_params_to_wire_syntax() {
        assert_eq!(convert_express_params("/users/:id"), "/users/{{id}}");
        assert_eq!(
            convert_express_params("/orgs/:orgId/users/:userId"),
            "/orgs/{{orgId}}/users/{{userId}}"
        );
        assert_eq!(convert_express_params("/health"), "/health");
    }

    #[test]
    fn parse_chained_route() {
        let code = r#"
router.route('/users')
    .get((req, res) => { res.json([]); })
    .post((req, res) => { res.json(req.body); });
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/users");
    }

    #[test]
    fn parse_typescript_routes() {
        let code = r#"
import { Router, Request, Response } from 'express';
const router: Router = Router();

router.get('/items', (req: Request, res: Response) => {
    res.json([]);
});

router.put('/items/:id', (req: Request, res: Response) => {
    const item = req.body;
    res.json(item);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/items");
        assert_eq!(endpoints[1].method, "PUT");
        assert_eq!(endpoints[1].route, "/items/{{id}}");
        assert_eq!(endpoints[1].body_type, Some("JSON".to_string()));
    }

    #[test]
    fn parse_routes_with_multiple_params() {
        let code = r#"
router.get('/orgs/:orgId/teams/:teamId/members', (req, res) => {
    res.json([]);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints[0].route,
            "/orgs/{{orgId}}/teams/{{teamId}}/members"
        );
        assert_eq!(endpoints[0].name, "GetOrgsByOrgIdTeamsByTeamIdMembers");
    }

    #[test]
    fn no_endpoints_in_non_route_code() {
        let code = r#"
const users = db.get('users');
const config = app.get('port');  // app.get with non-path string — should not match
console.log('Starting server...');
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        // app.get('port') doesn't match because 'port' doesn't start with '/'
        assert!(endpoints.is_empty());
    }

    #[test]
    fn scan_express_with_real_files() {
        let dir = TempDir::new().unwrap();
        let routes_dir = dir.path().join("routes");
        fs::create_dir_all(&routes_dir).unwrap();

        fs::write(
            routes_dir.join("users.js"),
            r#"
const router = require('express').Router();

router.get('/users', (req, res) => { res.json([]); });
router.post('/users', (req, res) => { res.json(req.body); });

module.exports = router;
"#,
        )
        .unwrap();

        fs::write(
            routes_dir.join("health.ts"),
            r#"
import { Router } from 'express';
const router = Router();

router.get('/health', (req, res) => { res.json({ ok: true }); });

export default router;
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_express(dir.path());
        assert_eq!(files_scanned, 2);
        assert_eq!(endpoints.len(), 3);
    }

    #[test]
    fn scan_express_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules/some-pkg");
        fs::create_dir_all(&nm).unwrap();
        fs::write(
            nm.join("index.js"),
            "router.get('/internal', (req, res) => {});",
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_express(dir.path());
        assert_eq!(files_scanned, 0);
        assert_eq!(endpoints.len(), 0);
    }

    #[test]
    fn scan_express_skips_test_files() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("users.test.js"),
            "router.get('/test-route', (req, res) => {});",
        )
        .unwrap();
        fs::write(
            dir.path().join("users.spec.ts"),
            "router.get('/spec-route', (req, res) => {});",
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_express(dir.path());
        assert_eq!(files_scanned, 0);
        assert_eq!(endpoints.len(), 0);
    }

    #[test]
    fn deduplicates_query_params() {
        let code = r#"
router.get('/search', (req, res) => {
    const q = req.query.q;
    if (!req.query.q) return res.status(400);
    res.json([]);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints[0].query_params.len(), 1);
    }

    #[test]
    fn derive_name_produces_readable_names() {
        assert_eq!(derive_name("GET", "/users"), "GetUsers");
        assert_eq!(derive_name("GET", "/users/:id"), "GetUsersById");
        assert_eq!(derive_name("POST", "/api/users"), "PostUsers");
        assert_eq!(
            derive_name("DELETE", "/api/v1/items/:id"),
            "DeleteItemsById"
        );
        assert_eq!(derive_name("GET", "/"), "GetRoot");
    }

    #[test]
    fn double_quoted_routes() {
        let code = r#"
router.get("/users", (req, res) => { res.json([]); });
router.post("/users/:id", (req, res) => { res.json(req.body); });
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[1].route, "/users/{{id}}");
    }

    #[test]
    fn extracts_destructured_body_fields() {
        let code = r#"
router.post('/users', (req, res) => {
    const { name, email, age } = req.body;
    res.json({ name, email, age });
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].body_type, Some("JSON".to_string()));
        assert_eq!(endpoints[0].body_fields.len(), 3);
        assert_eq!(endpoints[0].body_fields[0].0, "name");
        assert_eq!(endpoints[0].body_fields[1].0, "email");
        assert_eq!(endpoints[0].body_fields[2].0, "age");
    }

    #[test]
    fn extracts_dot_access_body_fields() {
        let code = r#"
router.post('/items', (req, res) => {
    const title = req.body.title;
    const price = req.body.price;
    res.json({ title, price });
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].body_fields.len(), 2);
        assert_eq!(endpoints[0].body_fields[0].0, "title");
        assert_eq!(endpoints[0].body_fields[1].0, "price");
    }

    #[test]
    fn no_body_fields_when_no_req_body() {
        let code = r#"
router.get('/items', (req, res) => {
    res.json([]);
});
"#;
        let endpoints = parse_express_routes(code, Path::new("routes/test.js"));
        assert_eq!(endpoints.len(), 1);
        assert!(endpoints[0].body_fields.is_empty());
        assert!(endpoints[0].body_type.is_none());
    }
}
