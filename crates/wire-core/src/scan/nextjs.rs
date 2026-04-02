use super::types::DiscoveredEndpoint;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

type ExtractedUsage = (
    Option<String>,
    Vec<(String, String)>,
    Vec<(String, String)>,
    Vec<(String, String)>,
);

/// Scan a Next.js project for API route handlers.
///
/// Supports both App Router (app/api/ with route.ts) and Pages Router
/// (pages/api/ with default exports). Dynamic segments [id] are
/// converted to Wire's {{id}} syntax.
pub fn scan_nextjs(project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    let files = collect_route_files(project_dir);
    let file_count = files.len();
    let mut endpoints = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let relative = file_path
            .strip_prefix(project_dir)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        if is_app_router_route(&relative) {
            endpoints.extend(parse_app_router(&content, &relative));
        } else if is_pages_router_route(&relative) {
            endpoints.extend(parse_pages_router(&content, &relative));
        }
    }

    (endpoints, file_count)
}

/// Collect route files from app/api/ and pages/api/ directories.
fn collect_route_files(project_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    // App Router: app/**/route.{ts,js}
    let app_dir = project_dir.join("app");
    if app_dir.is_dir() {
        collect_files_recursive(&app_dir, &mut files, 0);
    }
    // Also check src/app for src-dir layout
    let src_app_dir = project_dir.join("src").join("app");
    if src_app_dir.is_dir() {
        collect_files_recursive(&src_app_dir, &mut files, 0);
    }

    // Pages Router: pages/api/**/*.{ts,js,tsx,jsx}
    let pages_dir = project_dir.join("pages");
    if pages_dir.is_dir() {
        collect_files_recursive(&pages_dir, &mut files, 0);
    }
    let src_pages_dir = project_dir.join("src").join("pages");
    if src_pages_dir.is_dir() {
        collect_files_recursive(&src_pages_dir, &mut files, 0);
    }

    files
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 10 {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip common non-source directories
            if matches!(
                name,
                "node_modules"
                    | ".git"
                    | ".next"
                    | "dist"
                    | "build"
                    | ".wire"
                    | "coverage"
                    | "__tests__"
            ) {
                continue;
            }
            collect_files_recursive(&path, files, depth + 1);
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip test and type files
            if name.ends_with(".d.ts")
                || name.ends_with(".test.ts")
                || name.ends_with(".test.js")
                || name.ends_with(".spec.ts")
                || name.ends_with(".spec.js")
            {
                continue;
            }

            // Include JS/TS files
            if name.ends_with(".ts")
                || name.ends_with(".js")
                || name.ends_with(".tsx")
                || name.ends_with(".jsx")
            {
                files.push(path);
            }
        }
    }
}

/// Check if a relative path is an App Router route handler (route.ts/route.js).
fn is_app_router_route(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    normalized.contains("/api/") && {
        let filename = normalized.rsplit('/').next().unwrap_or("");
        filename.starts_with("route.")
    }
}

/// Check if a relative path is a Pages Router API route.
fn is_pages_router_route(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    normalized.contains("pages/api/")
}

/// Parse App Router route handlers (export const GET/POST/... from route.ts).
fn parse_app_router(content: &str, relative_path: &str) -> Vec<DiscoveredEndpoint> {
    let route = route_from_app_path(relative_path);
    let group = group_from_route(&route);

    // Match: export const GET, export async function GET, export function GET
    let export_re = Regex::new(
        r"(?m)export\s+(?:const|async\s+function|function)\s+(GET|POST|PUT|PATCH|DELETE)\b",
    )
    .unwrap();

    let mut endpoints = Vec::new();

    for cap in export_re.captures_iter(content) {
        let method = cap[1].to_uppercase();
        let name = derive_name(&method, &route);

        let (body_type, body_fields, query_params, headers) =
            extract_handler_usage(content, &method);

        endpoints.push(DiscoveredEndpoint {
            group: group.clone(),
            method,
            route: route.clone(),
            name,
            headers,
            query_params,
            body_type,
            body_fields,
            response_type: None,
            response_fields: Vec::new(),
        });
    }

    endpoints
}

/// Parse Pages Router API handlers (default export with req.method checks).
fn parse_pages_router(content: &str, relative_path: &str) -> Vec<DiscoveredEndpoint> {
    let route = route_from_pages_path(relative_path);
    let group = group_from_route(&route);

    let mut endpoints = Vec::new();

    // Check for explicit method checks: req.method === 'GET'
    let method_check_re = Regex::new(r#"(?m)req\.method\s*===?\s*['"](\w+)['"]"#).unwrap();

    let mut found_methods: HashSet<String> = HashSet::new();
    for cap in method_check_re.captures_iter(content) {
        let method = cap[1].to_uppercase();
        if matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            found_methods.insert(method);
        }
    }

    // If no explicit method checks, default to GET + POST (common Pages Router pattern)
    if found_methods.is_empty() {
        // Check if there's a default export (handler function)
        if content.contains("export default") {
            found_methods.insert("GET".to_string());
        }
    }

    for method in &found_methods {
        let name = derive_name(method, &route);
        let (body_type, body_fields, query_params, headers) =
            extract_pages_handler_usage(content, method);

        endpoints.push(DiscoveredEndpoint {
            group: group.clone(),
            method: method.clone(),
            route: route.clone(),
            name,
            headers,
            query_params,
            body_type,
            body_fields,
            response_type: None,
            response_fields: Vec::new(),
        });
    }

    endpoints
}

/// Convert App Router file path to URL route.
/// app/api/users/[id]/route.ts -> /api/users/{{id}}
fn route_from_app_path(relative_path: &str) -> String {
    let normalized = relative_path.replace('\\', "/");

    // Strip leading app/ or src/app/
    let path = normalized
        .trim_start_matches("src/")
        .trim_start_matches("app/");

    // Remove route.ts/route.js filename
    let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(path);

    // Remove route groups (parenthesized segments)
    let segments: Vec<&str> = dir
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('('))
        .collect();

    let route = format!("/{}", segments.join("/"));
    convert_nextjs_params(&route)
}

/// Convert Pages Router file path to URL route.
/// pages/api/posts/[id].ts -> /api/posts/{{id}}
fn route_from_pages_path(relative_path: &str) -> String {
    let normalized = relative_path.replace('\\', "/");

    // Strip leading pages/ or src/pages/
    let path = normalized
        .trim_start_matches("src/")
        .trim_start_matches("pages/");

    // Remove file extension
    let without_ext = path
        .trim_end_matches(".tsx")
        .trim_end_matches(".jsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".js");

    // Remove /index suffix (pages/api/users/index.ts -> /api/users)
    let without_index = without_ext.trim_end_matches("/index");

    let route = format!("/{without_index}");
    convert_nextjs_params(&route)
}

/// Convert Next.js dynamic segments to Wire variable syntax.
/// [id] -> {{id}}, [...slug] -> {{slug}}, [[...slug]] -> {{slug}}
fn convert_nextjs_params(route: &str) -> String {
    let re = Regex::new(r"\[\[?\.\.\.?(\w+)\]?\]|\[(\w+)\]").unwrap();
    re.replace_all(route, |caps: &regex::Captures| {
        if let Some(m) = caps.get(1) {
            format!("{{{{{}}}}}", m.as_str())
        } else if let Some(m) = caps.get(2) {
            format!("{{{{{}}}}}", m.as_str())
        } else {
            caps[0].to_string()
        }
    })
    .to_string()
}

/// Derive endpoint group from route path.
fn group_from_route(route: &str) -> String {
    route
        .trim_start_matches('/')
        .split('/')
        .find(|s| !s.is_empty() && *s != "api" && !s.starts_with("{{"))
        .unwrap_or("root")
        .to_lowercase()
}

/// Derive endpoint name from method and route.
fn derive_name(method: &str, route: &str) -> String {
    let parts: Vec<&str> = route
        .split('/')
        .filter(|s| !s.is_empty() && *s != "api" && !s.starts_with("v1") && !s.starts_with("v2"))
        .collect();

    if parts.is_empty() {
        return format!(
            "{}Root",
            method[..1].to_uppercase() + &method[1..].to_lowercase()
        );
    }

    let mut name_parts = vec![method[..1].to_uppercase() + &method[1..].to_lowercase()];
    for part in parts {
        let clean = part
            .trim_start_matches("{{")
            .trim_end_matches("}}")
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_start_matches("...");

        if clean.is_empty() {
            continue;
        }

        // PascalCase
        if clean.starts_with(|c: char| c.is_alphabetic()) {
            let pascal = clean[..1].to_uppercase() + &clean[1..];
            // Prefix with "By" for parameter segments
            if part.starts_with("{{") || part.starts_with('[') {
                name_parts.push(format!("By{pascal}"));
            } else {
                name_parts.push(pascal);
            }
        }
    }

    name_parts.join("")
}

/// Extract request usage from App Router handler body.
fn extract_handler_usage(content: &str, method: &str) -> ExtractedUsage {
    let mut body_type = None;
    let mut body_fields = Vec::new();
    let mut query_params = Vec::new();
    let mut headers = Vec::new();
    let mut seen_body = HashSet::new();
    let mut seen_query = HashSet::new();
    let mut seen_headers = HashSet::new();

    // Find the handler for this method
    let pattern = format!(r"export\s+(?:const|async\s+function|function)\s+{method}\b");
    let handler_re = Regex::new(&pattern).unwrap();

    let handler_body = if let Some(m) = handler_re.find(content) {
        let remaining = &content[m.start()..];
        let end = remaining.len().min(800);
        &remaining[..end]
    } else {
        return (body_type, body_fields, query_params, headers);
    };

    // Body: req.json() or request.json()
    if handler_body.contains(".json()") {
        body_type = Some("JSON".to_string());

        // Destructured: const { name, email } = await req.json()
        let destructure_re =
            Regex::new(r#"(?:const|let|var)\s+\{\s*([^}]+)\}\s*=\s*await\s+\w+\.json\(\)"#)
                .unwrap();
        if let Some(cap) = destructure_re.captures(handler_body) {
            for field in cap[1].split(',') {
                let field = field.trim().split(':').next().unwrap_or("").trim();
                if !field.is_empty() && seen_body.insert(field.to_string()) {
                    body_fields.push((field.to_string(), String::new()));
                }
            }
        }
    }

    // Query: req.nextUrl.searchParams.get('param') or url.searchParams.get('param')
    let query_re = Regex::new(r#"searchParams\.get\(\s*['"](\w+)['"]\s*\)"#).unwrap();
    for cap in query_re.captures_iter(handler_body) {
        let param = cap[1].to_string();
        if seen_query.insert(param.clone()) {
            query_params.push((param, String::new()));
        }
    }

    // Headers: req.headers.get('name') or headers().get('name')
    let header_re = Regex::new(r#"headers?(?:\(\))?\.get\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    for cap in header_re.captures_iter(handler_body) {
        let header = cap[1].to_string();
        if seen_headers.insert(header.clone()) {
            headers.push((header, String::new()));
        }
    }

    (body_type, body_fields, query_params, headers)
}

/// Extract request usage from Pages Router handler body.
fn extract_pages_handler_usage(content: &str, _method: &str) -> ExtractedUsage {
    let mut body_type = None;
    let mut body_fields = Vec::new();
    let mut query_params = Vec::new();
    let mut headers = Vec::new();
    let mut seen_body = HashSet::new();
    let mut seen_query = HashSet::new();
    let mut seen_headers = HashSet::new();

    // Body: req.body
    if content.contains("req.body") {
        body_type = Some("JSON".to_string());

        let destructure_re =
            Regex::new(r#"(?:const|let|var)\s+\{\s*([^}]+)\}\s*=\s*req\.body"#).unwrap();
        if let Some(cap) = destructure_re.captures(content) {
            for field in cap[1].split(',') {
                let field = field.trim().split(':').next().unwrap_or("").trim();
                if !field.is_empty() && seen_body.insert(field.to_string()) {
                    body_fields.push((field.to_string(), String::new()));
                }
            }
        }

        let dot_re = Regex::new(r#"req\.body\.(\w+)"#).unwrap();
        for cap in dot_re.captures_iter(content) {
            let field = cap[1].to_string();
            if seen_body.insert(field.clone()) {
                body_fields.push((field, String::new()));
            }
        }
    }

    // Query: req.query.param or const { param } = req.query
    let query_dot_re = Regex::new(r#"req\.query\.(\w+)"#).unwrap();
    for cap in query_dot_re.captures_iter(content) {
        let param = cap[1].to_string();
        if seen_query.insert(param.clone()) {
            query_params.push((param, String::new()));
        }
    }

    let destructure_query_re =
        Regex::new(r#"(?:const|let|var)\s+\{\s*([^}]+)\}\s*=\s*req\.query"#).unwrap();
    if let Some(cap) = destructure_query_re.captures(content) {
        for field in cap[1].split(',') {
            let field = field.trim().split(':').next().unwrap_or("").trim();
            if !field.is_empty() && seen_query.insert(field.to_string()) {
                query_params.push((field.to_string(), String::new()));
            }
        }
    }

    // Headers: req.headers['name']
    let header_re = Regex::new(r#"req\.headers\[['"]([^'"]+)['"]\]"#).unwrap();
    for cap in header_re.captures_iter(content) {
        let header = cap[1].to_string();
        if seen_headers.insert(header.clone()) {
            headers.push((header, String::new()));
        }
    }

    (body_type, body_fields, query_params, headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Parameter conversion ---

    #[test]
    fn convert_single_param() {
        assert_eq!(
            convert_nextjs_params("/api/users/[id]"),
            "/api/users/{{id}}"
        );
    }

    #[test]
    fn convert_multiple_params() {
        assert_eq!(
            convert_nextjs_params("/api/users/[userId]/posts/[postId]"),
            "/api/users/{{userId}}/posts/{{postId}}"
        );
    }

    #[test]
    fn convert_catch_all() {
        assert_eq!(
            convert_nextjs_params("/api/docs/[...slug]"),
            "/api/docs/{{slug}}"
        );
    }

    #[test]
    fn convert_optional_catch_all() {
        assert_eq!(
            convert_nextjs_params("/api/docs/[[...slug]]"),
            "/api/docs/{{slug}}"
        );
    }

    #[test]
    fn no_params_unchanged() {
        assert_eq!(convert_nextjs_params("/api/users"), "/api/users");
    }

    // --- App Router path to route ---

    #[test]
    fn app_router_simple_route() {
        assert_eq!(route_from_app_path("app/api/users/route.ts"), "/api/users");
    }

    #[test]
    fn app_router_dynamic_route() {
        assert_eq!(
            route_from_app_path("app/api/users/[id]/route.ts"),
            "/api/users/{{id}}"
        );
    }

    #[test]
    fn app_router_route_group_stripped() {
        assert_eq!(
            route_from_app_path("app/(auth)/api/login/route.ts"),
            "/api/login"
        );
    }

    #[test]
    fn app_router_src_layout() {
        assert_eq!(
            route_from_app_path("src/app/api/items/route.ts"),
            "/api/items"
        );
    }

    // --- Pages Router path to route ---

    #[test]
    fn pages_router_simple_route() {
        assert_eq!(route_from_pages_path("pages/api/hello.ts"), "/api/hello");
    }

    #[test]
    fn pages_router_dynamic_route() {
        assert_eq!(
            route_from_pages_path("pages/api/posts/[id].ts"),
            "/api/posts/{{id}}"
        );
    }

    #[test]
    fn pages_router_index_file() {
        assert_eq!(
            route_from_pages_path("pages/api/users/index.ts"),
            "/api/users"
        );
    }

    // --- Name derivation ---

    #[test]
    fn derive_name_simple() {
        assert_eq!(derive_name("GET", "/api/users"), "GetUsers");
    }

    #[test]
    fn derive_name_with_param() {
        assert_eq!(derive_name("GET", "/api/users/{{id}}"), "GetUsersById");
    }

    #[test]
    fn derive_name_root() {
        assert_eq!(derive_name("GET", "/api"), "GetRoot");
    }

    // --- Group derivation ---

    #[test]
    fn group_from_simple_route() {
        assert_eq!(group_from_route("/api/users"), "users");
    }

    #[test]
    fn group_from_nested_route() {
        assert_eq!(group_from_route("/api/users/{{id}}/posts"), "users");
    }

    #[test]
    fn group_from_root() {
        assert_eq!(group_from_route("/api"), "root");
    }

    // --- App Router parsing ---

    #[test]
    fn parse_app_router_exports() {
        let content = r#"
import { NextResponse } from 'next/server';

export async function GET(req: Request) {
    const { searchParams } = new URL(req.url);
    const page = searchParams.get('page');
    return NextResponse.json({ users: [] });
}

export async function POST(req: Request) {
    const { name, email } = await req.json();
    return NextResponse.json({ id: 1, name, email }, { status: 201 });
}
"#;
        let endpoints = parse_app_router(content, "app/api/users/route.ts");
        assert_eq!(endpoints.len(), 2);

        let get = endpoints.iter().find(|e| e.method == "GET").unwrap();
        assert_eq!(get.route, "/api/users");
        assert_eq!(get.group, "users");
        assert!(get.query_params.iter().any(|(k, _)| k == "page"));

        let post = endpoints.iter().find(|e| e.method == "POST").unwrap();
        assert_eq!(post.body_type, Some("JSON".to_string()));
        assert!(post.body_fields.iter().any(|(k, _)| k == "name"));
        assert!(post.body_fields.iter().any(|(k, _)| k == "email"));
    }

    #[test]
    fn parse_app_router_const_exports() {
        let content = r#"
export const GET = async (req: Request) => {
    return Response.json({ ok: true });
}

export const DELETE = async (req: Request) => {
    return new Response(null, { status: 204 });
}
"#;
        let endpoints = parse_app_router(content, "app/api/items/[id]/route.ts");
        assert_eq!(endpoints.len(), 2);
        assert!(endpoints.iter().any(|e| e.method == "GET"));
        assert!(endpoints.iter().any(|e| e.method == "DELETE"));
        assert_eq!(endpoints[0].route, "/api/items/{{id}}");
    }

    #[test]
    fn parse_app_router_with_headers() {
        let content = r#"
export async function GET(req: Request) {
    const token = req.headers.get('authorization');
    return Response.json({ ok: true });
}
"#;
        let endpoints = parse_app_router(content, "app/api/protected/route.ts");
        assert_eq!(endpoints.len(), 1);
        assert!(endpoints[0]
            .headers
            .iter()
            .any(|(k, _)| k == "authorization"));
    }

    // --- Pages Router parsing ---

    #[test]
    fn parse_pages_router_with_method_checks() {
        let content = r#"
export default function handler(req, res) {
    if (req.method === 'GET') {
        const { page } = req.query;
        res.json({ users: [] });
    } else if (req.method === 'POST') {
        const { name, email } = req.body;
        res.status(201).json({ id: 1 });
    }
}
"#;
        let endpoints = parse_pages_router(content, "pages/api/users.ts");
        assert_eq!(endpoints.len(), 2);

        let get = endpoints.iter().find(|e| e.method == "GET").unwrap();
        assert!(get.query_params.iter().any(|(k, _)| k == "page"));

        let post = endpoints.iter().find(|e| e.method == "POST").unwrap();
        assert_eq!(post.body_type, Some("JSON".to_string()));
        assert!(post.body_fields.iter().any(|(k, _)| k == "name"));
    }

    #[test]
    fn parse_pages_router_default_export_only() {
        let content = r#"
export default function handler(req, res) {
    res.json({ hello: 'world' });
}
"#;
        let endpoints = parse_pages_router(content, "pages/api/hello.ts");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/hello");
    }

    // --- File detection ---

    #[test]
    fn is_app_router_route_detection() {
        assert!(is_app_router_route("app/api/users/route.ts"));
        assert!(is_app_router_route("src/app/api/items/route.js"));
        assert!(!is_app_router_route("app/api/users/utils.ts"));
        assert!(!is_app_router_route("pages/api/hello.ts"));
    }

    #[test]
    fn is_pages_router_route_detection() {
        assert!(is_pages_router_route("pages/api/hello.ts"));
        assert!(is_pages_router_route("src/pages/api/users/[id].ts"));
        assert!(!is_pages_router_route("app/api/users/route.ts"));
    }

    // --- Integration test with real files ---

    #[test]
    fn scan_nextjs_with_real_files() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();

        // Create package.json with next dependency
        std::fs::write(
            project.join("package.json"),
            r#"{"dependencies": {"next": "14.0.0"}}"#,
        )
        .unwrap();

        // App Router route
        let app_api = project.join("app/api/users");
        std::fs::create_dir_all(&app_api).unwrap();
        std::fs::write(
            app_api.join("route.ts"),
            r#"
export async function GET(req: Request) {
    return Response.json({ users: [] });
}

export async function POST(req: Request) {
    const { name } = await req.json();
    return Response.json({ id: 1, name }, { status: 201 });
}
"#,
        )
        .unwrap();

        // Pages Router route
        let pages_api = project.join("pages/api");
        std::fs::create_dir_all(&pages_api).unwrap();
        std::fs::write(
            pages_api.join("hello.ts"),
            r#"
export default function handler(req, res) {
    res.json({ hello: 'world' });
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_nextjs(project);
        assert!(files_scanned >= 2);
        assert!(endpoints.len() >= 3); // GET users, POST users, GET hello

        let get_users = endpoints
            .iter()
            .find(|e| e.method == "GET" && e.route == "/api/users")
            .unwrap();
        assert_eq!(get_users.group, "users");

        let hello = endpoints.iter().find(|e| e.route == "/api/hello").unwrap();
        assert_eq!(hello.method, "GET");
    }

    #[test]
    fn scan_skips_node_modules_and_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();

        // Should be scanned
        let app_api = project.join("app/api/ok");
        std::fs::create_dir_all(&app_api).unwrap();
        std::fs::write(
            app_api.join("route.ts"),
            "export async function GET() { return Response.json({}); }",
        )
        .unwrap();

        // Should be skipped (node_modules)
        let nm = project.join("node_modules/some-pkg/app/api/bad");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::write(nm.join("route.ts"), "export async function GET() {}").unwrap();

        // Should be skipped (test file)
        std::fs::write(
            app_api.join("route.test.ts"),
            "export async function GET() {}",
        )
        .unwrap();

        let (endpoints, _) = scan_nextjs(project);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].route, "/api/ok");
    }
}
