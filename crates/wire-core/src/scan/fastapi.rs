use super::types::DiscoveredEndpoint;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Scan a FastAPI project for HTTP endpoints.
///
/// Detects `@app.get`, `@router.post`, etc. decorators and extracts
/// path parameters, query parameters (from function signatures),
/// request body (from Pydantic model type hints), and response models.
pub fn scan_fastapi(project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    let files = collect_python_files(project_dir);
    let file_count = files.len();
    let mut endpoints = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if !has_fastapi_imports(&content) {
            continue;
        }

        let router_prefix = extract_router_prefix(&content);
        let file_endpoints = parse_fastapi_endpoints(&content, &router_prefix);
        endpoints.extend(file_endpoints);
    }

    (endpoints, file_count)
}

/// Collect .py files, skipping virtual environments, caches, and test directories.
fn collect_python_files(project_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(project_dir, &mut files, 20);
    files
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>, depth: u32) {
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
                "__pycache__"
                    | ".venv"
                    | "venv"
                    | ".tox"
                    | ".git"
                    | ".wire"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | ".mypy_cache"
                    | ".pytest_cache"
                    | ".eggs"
            ) {
                continue;
            }
            collect_files_recursive(&path, files, depth - 1);
        } else if path.extension().is_some_and(|e| e == "py") {
            // Skip test files
            let fname = name.to_string();
            if fname.starts_with("test_") || fname.ends_with("_test.py") || fname == "conftest.py" {
                continue;
            }
            files.push(path);
        }
    }
}

/// Check if a Python file contains FastAPI imports.
fn has_fastapi_imports(content: &str) -> bool {
    content.contains("from fastapi import") || content.contains("import fastapi")
}

/// Extract APIRouter prefix if present.
///
/// Matches patterns like:
/// - `router = APIRouter(prefix="/api/v1")`
/// - `router = APIRouter(prefix='/users')`
fn extract_router_prefix(content: &str) -> String {
    let re = Regex::new(r#"APIRouter\s*\([^)]*prefix\s*=\s*["']([^"']+)["']"#).unwrap();
    if let Some(cap) = re.captures(content) {
        return cap[1].to_string();
    }
    String::new()
}

/// Parse FastAPI endpoint decorators from file content.
fn parse_fastapi_endpoints(content: &str, router_prefix: &str) -> Vec<DiscoveredEndpoint> {
    let mut endpoints = Vec::new();

    // Match @app.get("/path") or @router.post("/path") etc.
    // Uses [\s\S]*? to support multiline decorators
    let decorator_re = Regex::new(
        r#"(?m)@(\w+)\.(get|post|put|patch|delete)\s*\(\s*["']([^"']+)["']([\s\S]*?)\)"#,
    )
    .unwrap();

    // Also match decorators with no path (root endpoint): @app.get()
    let decorator_no_path_re =
        Regex::new(r#"(?m)@(\w+)\.(get|post|put|patch|delete)\s*\(\s*\)"#).unwrap();

    // Collect path param names from route for filtering
    let path_param_re = Regex::new(r"\{(\w+)}").unwrap();

    for cap in decorator_re.captures_iter(content) {
        let method = cap[2].to_uppercase();
        let path = &cap[3];
        let kwargs = cap.get(4).map(|m| m.as_str()).unwrap_or("");

        let full_route = build_route(router_prefix, path);
        let wire_route = convert_python_params(&full_route);
        let name = derive_name(&method, &wire_route);
        let group = group_from_route(&wire_route);

        // Collect path param names to exclude from query params
        let path_params: HashSet<String> = path_param_re
            .captures_iter(path)
            .map(|c| c[1].to_string())
            .collect();

        // Extract response_model from decorator kwargs
        let response_type = extract_response_model(kwargs);

        // Extract function signature parameters
        let decorator_end = cap.get(0).unwrap().end();
        let func_sig = extract_function_signature(content, decorator_end);
        let (query_params, body_type, headers) = extract_params(&func_sig, &path_params);

        endpoints.push(DiscoveredEndpoint {
            group,
            method,
            route: wire_route,
            name,
            headers,
            query_params,
            body_type,
            body_fields: Vec::new(),
            response_type,
            response_fields: Vec::new(),
        });
    }

    // Track positions matched by decorator_re to avoid duplicates
    let matched_positions: HashSet<usize> = decorator_re
        .captures_iter(content)
        .map(|c| c.get(0).unwrap().start())
        .collect();

    for cap in decorator_no_path_re.captures_iter(content) {
        // Skip if this position was already matched by the path-capturing regex
        if matched_positions.contains(&cap.get(0).unwrap().start()) {
            continue;
        }

        let method = cap[2].to_uppercase();

        let full_route = if router_prefix.is_empty() {
            "/".to_string()
        } else {
            router_prefix.to_string()
        };
        let wire_route = convert_python_params(&full_route);
        let name = derive_name(&method, &wire_route);
        let group = group_from_route(&wire_route);

        let decorator_end = cap.get(0).unwrap().end();
        let func_sig = extract_function_signature(content, decorator_end);
        let empty_path_params = HashSet::new();
        let (query_params, body_type, headers) = extract_params(&func_sig, &empty_path_params);

        endpoints.push(DiscoveredEndpoint {
            group,
            method,
            route: wire_route,
            name,
            headers,
            query_params,
            body_type,
            body_fields: Vec::new(),
            response_type: None,
            response_fields: Vec::new(),
        });
    }

    endpoints
}

/// Combine router prefix and endpoint path.
fn build_route(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if prefix.is_empty() {
        path
    } else {
        format!("{prefix}{path}")
    }
}

/// Convert Python path parameters `{param}` to Wire `{{param}}` syntax.
fn convert_python_params(route: &str) -> String {
    let re = Regex::new(r"\{(\w+)}").unwrap();
    re.replace_all(route, "{{$1}}").to_string()
}

/// Extract the function signature from `def func_name(...)`.
/// Limits search to 300 chars after decorator to avoid matching unrelated functions.
fn extract_function_signature(content: &str, start: usize) -> String {
    let remaining = &content[start..];
    let search_window = &remaining[..remaining.len().min(300)];

    // Find `def ` followed by function name and opening paren
    let def_pos = match search_window.find("def ") {
        Some(p) => p,
        None => return String::new(),
    };

    let after_def = &remaining[def_pos..];
    let open_paren = match after_def.find('(') {
        Some(p) => p,
        None => return String::new(),
    };

    // Find the matching closing paren (handle nested parens)
    let sig_start = def_pos + open_paren + 1;
    let sig_content = &remaining[sig_start..];
    let mut depth = 1;
    let mut end = 0;
    for (i, c) in sig_content.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    sig_content[..end].to_string()
}

/// Extract response_model from decorator kwargs.
fn extract_response_model(kwargs: &str) -> Option<String> {
    let re = Regex::new(r#"response_model\s*=\s*(\w+)"#).unwrap();
    re.captures(kwargs).map(|c| c[1].to_string())
}

/// Extract parameter info from a FastAPI function signature.
///
/// FastAPI convention:
/// - Path params: matched by name against route `{param}` — skipped via `path_params` set
/// - Query params: `param: type = default` or `param: type = Query(...)`
/// - Body params: `body: PydanticModel` (capitalized type hint, not a builtin)
/// - Headers: `x_token: str = Header(...)` or annotated with Header
///
/// Extracted parameter info: (query_params, body_type, headers)
type ExtractedParams = (Vec<(String, String)>, Option<String>, Vec<(String, String)>);

/// Returns (query_params, body_type, headers).
fn extract_params(sig: &str, path_params: &HashSet<String>) -> ExtractedParams {
    let mut query_params = Vec::new();
    let mut body_type = None;
    let mut headers = Vec::new();
    let mut seen_query = HashSet::new();
    let mut seen_headers = HashSet::new();

    // Python builtins and common non-body types
    let builtin_types = [
        "str", "int", "float", "bool", "list", "dict", "set", "tuple", "bytes", "None", "Optional",
        "List", "Dict", "Set", "Tuple", "Any", "Union",
    ];

    // Split params by comma, handling nested types like Optional[str]
    let params = split_params(sig);

    for param in &params {
        let param = param.trim();
        if param.is_empty() || param == "self" || param == "cls" {
            continue;
        }

        // Skip request/response objects and dependency injection
        if param.contains("Request") && !param.contains("Body") {
            continue;
        }
        if param.contains("Response") {
            continue;
        }
        if param.contains("Depends(") {
            continue;
        }

        // Check for Header(...) annotation
        if param.contains("Header(") {
            if let Some(name) = extract_param_name(param) {
                if seen_headers.insert(name.clone()) {
                    headers.push((name, String::new()));
                }
            }
            continue;
        }

        // Check for Query(...) annotation — explicit query param
        if param.contains("Query(") {
            if let Some(name) = extract_param_name(param) {
                if seen_query.insert(name.clone()) {
                    query_params.push((name, String::new()));
                }
            }
            continue;
        }

        // Check for Body(...) annotation — explicit body param
        if param.contains("Body(") {
            if let Some(name) = extract_param_name(param) {
                body_type = Some(name);
            }
            continue;
        }

        // Parse `name: Type` or `name: Type = default`
        let parts: Vec<&str> = param.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let name = parts[0].trim().to_string();
        let type_part = parts[1].split('=').next().unwrap_or("").trim();

        // Skip if no type annotation
        if type_part.is_empty() {
            continue;
        }

        // Extract the base type name (strip Optional[], List[], etc.)
        let base_type = extract_base_type(type_part);

        // Pydantic model: capitalized, not a builtin
        if !base_type.is_empty()
            && base_type.chars().next().is_some_and(|c| c.is_uppercase())
            && !builtin_types.contains(&base_type.as_str())
        {
            body_type = Some(base_type);
            continue;
        }

        // Skip path parameters (already in the route as {{param}})
        if path_params.contains(&name) {
            continue;
        }

        // Otherwise it's a query parameter (if it has a default value or simple type)
        if seen_query.insert(name.clone()) {
            query_params.push((name, type_part.to_string()));
        }
    }

    (query_params, body_type, headers)
}

/// Split function parameters by commas, respecting nested brackets.
fn split_params(sig: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in sig.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                params.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        params.push(current.trim().to_string());
    }
    params
}

/// Extract parameter name from a `name: Type = ...` string.
fn extract_param_name(param: &str) -> Option<String> {
    let name = param.split(':').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Extract the base type from a type annotation, stripping Optional[], List[], etc.
fn extract_base_type(type_str: &str) -> String {
    let s = type_str.trim();
    // Handle Optional[Type]
    if let Some(inner) = s
        .strip_prefix("Optional[")
        .and_then(|s| s.strip_suffix(']'))
    {
        return extract_base_type(inner);
    }
    // Handle List[Type]
    if let Some(inner) = s.strip_prefix("List[").and_then(|s| s.strip_suffix(']')) {
        return extract_base_type(inner);
    }
    // Handle list[Type] (Python 3.9+)
    if let Some(inner) = s.strip_prefix("list[").and_then(|s| s.strip_suffix(']')) {
        return extract_base_type(inner);
    }
    // Return the type name itself
    s.split('[').next().unwrap_or(s).trim().to_string()
}

/// Derive a human-readable name from HTTP method and route.
fn derive_name(method: &str, route: &str) -> String {
    let parts: Vec<&str> = route
        .split('/')
        .filter(|s| !s.is_empty() && *s != "api" && *s != "v1" && *s != "v2" && *s != "v3")
        .collect();

    let mut name = method
        .chars()
        .next()
        .unwrap_or('G')
        .to_uppercase()
        .to_string()
        + &method[1..].to_lowercase();

    for part in parts {
        if part.starts_with("{{") {
            let param = part.trim_start_matches("{{").trim_end_matches("}}");
            name.push_str("By");
            name.push_str(&snake_to_pascal(param));
        } else {
            name.push_str(&snake_to_pascal(part));
        }
    }

    if name.len() <= method.len() {
        name.push_str("Root");
    }

    name
}

/// Derive group from route.
fn group_from_route(route: &str) -> String {
    route
        .trim_start_matches('/')
        .split('/')
        .find(|s| {
            !s.is_empty()
                && *s != "api"
                && !s.starts_with("{{")
                && *s != "v1"
                && *s != "v2"
                && *s != "v3"
        })
        .unwrap_or("root")
        .to_lowercase()
}

/// Convert snake_case to PascalCase: "user_id" -> "UserId"
fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(capitalize)
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn basic_get_endpoint() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
def get_users():
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[0].name, "GetUsers");
        assert_eq!(endpoints[0].group, "users");
    }

    #[test]
    fn post_with_body() {
        let content = r#"
from fastapi import FastAPI
from pydantic import BaseModel

class CreateUser(BaseModel):
    name: str
    email: str

app = FastAPI()

@app.post("/users")
def create_user(user: CreateUser):
    return user
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "POST");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[0].body_type, Some("CreateUser".to_string()));
    }

    #[test]
    fn path_parameters() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"id": user_id}

@app.get("/users/{user_id}/posts/{post_id}")
def get_user_post(user_id: int, post_id: int):
    return {}
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].route, "/users/{{user_id}}");
        assert_eq!(endpoints[0].name, "GetUsersByUserId");
        // Path params should NOT appear in query_params
        assert!(
            endpoints[0].query_params.is_empty(),
            "path params should not leak into query_params"
        );
        assert_eq!(endpoints[1].route, "/users/{{user_id}}/posts/{{post_id}}");
        assert!(endpoints[1].query_params.is_empty());
    }

    #[test]
    fn query_parameters() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/items")
def list_items(skip: int = 0, limit: int = 10, q: str = ""):
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].query_params.len(), 3);
        assert_eq!(endpoints[0].query_params[0].0, "skip");
        assert_eq!(endpoints[0].query_params[0].1, "int");
        assert_eq!(endpoints[0].query_params[1].0, "limit");
        assert_eq!(endpoints[0].query_params[2].0, "q");
    }

    #[test]
    fn query_with_explicit_query_annotation() {
        let content = r#"
from fastapi import FastAPI, Query

app = FastAPI()

@app.get("/search")
def search(q: str = Query(..., min_length=1), page: int = Query(default=1)):
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].query_params.len(), 2);
        assert_eq!(endpoints[0].query_params[0].0, "q");
        assert_eq!(endpoints[0].query_params[1].0, "page");
    }

    #[test]
    fn header_extraction() {
        let content = r#"
from fastapi import FastAPI, Header

app = FastAPI()

@app.get("/me")
def get_current_user(x_token: str = Header(...)):
    return {}
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].headers.len(), 1);
        assert_eq!(endpoints[0].headers[0].0, "x_token");
    }

    #[test]
    fn response_model() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users/{user_id}", response_model=UserResponse)
def get_user(user_id: int):
    return {}
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].response_type, Some("UserResponse".to_string()));
    }

    #[test]
    fn router_with_prefix() {
        let content = r#"
from fastapi import APIRouter

router = APIRouter(prefix="/api/v1")

@router.get("/items")
def list_items():
    return []

@router.post("/items")
def create_item(item: ItemCreate):
    return item
"#;
        let prefix = extract_router_prefix(content);
        assert_eq!(prefix, "/api/v1");
        let endpoints = parse_fastapi_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/v1/items");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/api/v1/items");
        assert_eq!(endpoints[1].body_type, Some("ItemCreate".to_string()));
    }

    #[test]
    fn all_http_methods() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/items")
def list_items():
    return []

@app.post("/items")
def create_item(item: Item):
    return item

@app.put("/items/{id}")
def update_item(id: int, item: Item):
    return item

@app.patch("/items/{id}")
def patch_item(id: int, item: Item):
    return item

@app.delete("/items/{id}")
def delete_item(id: int):
    return None
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 5);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/items");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/items");
        assert_eq!(endpoints[2].method, "PUT");
        assert_eq!(endpoints[2].route, "/items/{{id}}");
        assert_eq!(endpoints[3].method, "PATCH");
        assert_eq!(endpoints[3].route, "/items/{{id}}");
        assert_eq!(endpoints[4].method, "DELETE");
        assert_eq!(endpoints[4].route, "/items/{{id}}");
    }

    #[test]
    fn multiline_decorator() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post(
    "/users",
    response_model=UserResponse,
    status_code=201
)
def create_user(user: UserCreate):
    return user
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "POST");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[0].response_type, Some("UserResponse".to_string()));
        assert_eq!(endpoints[0].body_type, Some("UserCreate".to_string()));
    }

    #[test]
    fn path_params_with_query_params_mixed() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users/{user_id}/posts")
def get_user_posts(user_id: int, skip: int = 0, limit: int = 10):
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].route, "/users/{{user_id}}/posts");
        // user_id is a path param, should NOT be in query_params
        assert_eq!(endpoints[0].query_params.len(), 2);
        assert_eq!(endpoints[0].query_params[0].0, "skip");
        assert_eq!(endpoints[0].query_params[1].0, "limit");
    }

    #[test]
    fn convert_python_params_basic() {
        assert_eq!(convert_python_params("/users/{id}"), "/users/{{id}}");
        assert_eq!(
            convert_python_params("/users/{user_id}/posts/{post_id}"),
            "/users/{{user_id}}/posts/{{post_id}}"
        );
        assert_eq!(convert_python_params("/users"), "/users");
    }

    #[test]
    fn derive_name_patterns() {
        assert_eq!(derive_name("GET", "/api/users"), "GetUsers");
        assert_eq!(
            derive_name("GET", "/api/users/{{user_id}}"),
            "GetUsersByUserId"
        );
        assert_eq!(derive_name("POST", "/items"), "PostItems");
        assert_eq!(derive_name("GET", "/"), "GetRoot");
    }

    #[test]
    fn group_from_route_patterns() {
        assert_eq!(group_from_route("/api/users"), "users");
        assert_eq!(group_from_route("/api/users/{{id}}"), "users");
        assert_eq!(group_from_route("/health"), "health");
        assert_eq!(group_from_route("/"), "root");
        assert_eq!(group_from_route("/api/v1/items"), "items");
    }

    #[test]
    fn extract_base_type_strips_wrappers() {
        assert_eq!(extract_base_type("str"), "str");
        assert_eq!(extract_base_type("Optional[UserModel]"), "UserModel");
        assert_eq!(extract_base_type("List[Item]"), "Item");
        assert_eq!(extract_base_type("list[Item]"), "Item");
        assert_eq!(extract_base_type("Dict[str, Any]"), "Dict");
    }

    #[test]
    fn split_params_handles_nested() {
        let params = split_params("a: int, b: Optional[str] = None, c: List[int] = []");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], "a: int");
        assert_eq!(params[1], "b: Optional[str] = None");
        assert_eq!(params[2], "c: List[int] = []");
    }

    #[test]
    fn non_fastapi_file_ignored() {
        let content = r#"
import flask

app = flask.Flask(__name__)

@app.route("/users")
def get_users():
    return []
"#;
        assert!(!has_fastapi_imports(content));
    }

    #[test]
    fn skip_depends_params() {
        let content = r#"
from fastapi import FastAPI, Depends

app = FastAPI()

@app.get("/users")
def get_users(db = Depends(get_db), skip: int = 0):
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        // Should have skip as query param but NOT db (Depends)
        assert_eq!(endpoints[0].query_params.len(), 1);
        assert_eq!(endpoints[0].query_params[0].0, "skip");
    }

    #[test]
    fn async_def_endpoints() {
        let content = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
async def get_users():
    return []
"#;
        let endpoints = parse_fastapi_endpoints(content, "");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/users");
    }

    #[test]
    fn integration_scan_fastapi_project() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "fastapi==0.104.0\nuvicorn==0.24.0\n",
        )
        .unwrap();

        fs::write(
            dir.path().join("main.py"),
            r#"
from fastapi import FastAPI
from pydantic import BaseModel

class UserCreate(BaseModel):
    name: str
    email: str

app = FastAPI()

@app.get("/users")
def list_users(skip: int = 0, limit: int = 10):
    return []

@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {}

@app.post("/users", response_model=UserCreate)
def create_user(user: UserCreate):
    return user

@app.delete("/users/{user_id}")
def delete_user(user_id: int):
    return None
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_fastapi(dir.path());
        assert_eq!(files_scanned, 1);
        assert_eq!(endpoints.len(), 4);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/users");
        assert_eq!(endpoints[0].query_params.len(), 2);
        assert_eq!(endpoints[0].query_params[0].0, "skip");
        assert_eq!(endpoints[0].query_params[1].0, "limit");

        assert_eq!(endpoints[1].method, "GET");
        assert_eq!(endpoints[1].route, "/users/{{user_id}}");

        assert_eq!(endpoints[2].method, "POST");
        assert_eq!(endpoints[2].route, "/users");
        assert_eq!(endpoints[2].body_type, Some("UserCreate".to_string()));
        assert_eq!(endpoints[2].response_type, Some("UserCreate".to_string()));

        assert_eq!(endpoints[3].method, "DELETE");
        assert_eq!(endpoints[3].route, "/users/{{user_id}}");
    }

    #[test]
    fn integration_skip_venv_and_pycache() {
        let dir = TempDir::new().unwrap();

        let venv = dir.path().join(".venv/lib/python3.11/site-packages");
        fs::create_dir_all(&venv).unwrap();
        fs::write(
            venv.join("some_module.py"),
            "from fastapi import FastAPI\n@app.get('/fake')\ndef fake(): pass\n",
        )
        .unwrap();

        let pycache = dir.path().join("__pycache__");
        fs::create_dir_all(&pycache).unwrap();
        fs::write(
            pycache.join("main.cpython-311.py"),
            "from fastapi import FastAPI\n",
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_fastapi(dir.path());
        assert_eq!(files_scanned, 0);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn integration_skip_test_files() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("test_main.py"),
            "from fastapi import FastAPI\n@app.get('/test')\ndef test(): pass\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("conftest.py"),
            "from fastapi import FastAPI\n",
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_fastapi(dir.path());
        assert_eq!(files_scanned, 0);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn extract_router_prefix_patterns() {
        assert_eq!(
            extract_router_prefix(r#"router = APIRouter(prefix="/api/v1")"#),
            "/api/v1"
        );
        assert_eq!(
            extract_router_prefix(r#"router = APIRouter(prefix='/users', tags=['users'])"#),
            "/users"
        );
        assert_eq!(
            extract_router_prefix(r#"router = APIRouter(tags=['users'])"#),
            ""
        );
    }

    #[test]
    fn build_route_combines_prefix_and_path() {
        assert_eq!(build_route("/api", "/users"), "/api/users");
        assert_eq!(build_route("/api/", "/users"), "/api/users");
        assert_eq!(build_route("", "/users"), "/users");
        assert_eq!(build_route("/api/v1", "/items/{id}"), "/api/v1/items/{id}");
    }
}
