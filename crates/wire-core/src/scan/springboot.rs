use super::types::DiscoveredEndpoint;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Scan a Spring Boot project for HTTP endpoints.
///
/// Detects @GetMapping, @PostMapping, @PutMapping, @PatchMapping, @DeleteMapping,
/// and @RequestMapping annotations on controller classes. Supports both Java and Kotlin.
pub fn scan_springboot(project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    let files = collect_source_files(project_dir);
    let file_count = files.len();
    let mut endpoints = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if !is_controller(&content) {
            continue;
        }

        let class_prefix = extract_class_request_mapping(&content);
        let class_endpoints = parse_controller_endpoints(&content, &class_prefix);
        endpoints.extend(class_endpoints);
    }

    (endpoints, file_count)
}

/// Collect .java and .kt files, skipping build and test directories.
fn collect_source_files(project_dir: &Path) -> Vec<PathBuf> {
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
                "build"
                    | "target"
                    | ".gradle"
                    | ".mvn"
                    | "node_modules"
                    | ".git"
                    | ".wire"
                    | ".idea"
            ) {
                continue;
            }
            collect_files_recursive(&path, files, depth - 1);
        } else if path.extension().is_some_and(|e| e == "java" || e == "kt") {
            // Skip test source files (Maven/Gradle convention: src/test/)
            let path_str = path.to_string_lossy();
            if path_str.contains("/src/test/") || path_str.contains("\\src\\test\\") {
                continue;
            }
            files.push(path);
        }
    }
}

/// Check if source file contains a Spring controller annotation.
fn is_controller(content: &str) -> bool {
    content.contains("@RestController") || content.contains("@Controller")
}

/// Extract class-level @RequestMapping path prefix.
///
/// Handles:
/// - `@RequestMapping("/api/v1")`
/// - `@RequestMapping(value = "/api/v1")`
/// - `@RequestMapping("/api/v1")` with single or double quotes
fn extract_class_request_mapping(content: &str) -> String {
    let re = Regex::new(
        r#"(?m)@RequestMapping\s*\(\s*(?:value\s*=\s*)?["']([^"']+)["']\s*(?:,\s*[^)]*)?(?:\))"#,
    )
    .unwrap();

    // Find the @RequestMapping that comes before a class declaration
    // We look for @RequestMapping followed by class/object keyword
    for cap in re.captures_iter(content) {
        let match_end = cap.get(0).unwrap().end();
        // Check if a class definition follows within ~200 chars
        let remaining = &content[match_end..];
        let scan_window = &remaining[..remaining.len().min(200)];
        if scan_window.contains("class ") || scan_window.contains("object ") {
            let path = cap[1].to_string();
            return normalize_path(&path);
        }
    }

    String::new()
}

/// Parse method-level mapping annotations from a controller.
fn parse_controller_endpoints(content: &str, class_prefix: &str) -> Vec<DiscoveredEndpoint> {
    let mut endpoints = Vec::new();

    // Match shorthand mappings: @GetMapping, @PostMapping, etc.
    let shorthand_re = Regex::new(
        r#"(?m)@(Get|Post|Put|Patch|Delete)Mapping\s*(?:\(\s*(?:value\s*=\s*|path\s*=\s*)?["']([^"']+)["'](?:\s*,\s*[^)]*)?(?:\))\s*|\(\s*\)\s*|\s+)"#,
    )
    .unwrap();

    for cap in shorthand_re.captures_iter(content) {
        let method = match &cap[1] {
            "Get" => "GET",
            "Post" => "POST",
            "Put" => "PUT",
            "Patch" => "PATCH",
            "Delete" => "DELETE",
            _ => continue,
        };

        let path = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        let full_route = build_route(class_prefix, &path);
        let wire_route = convert_spring_params(&full_route);
        let name = derive_name(method, &wire_route);
        let group = group_from_route(&wire_route);

        // Extract parameter info from the method signature following the annotation
        let match_end = cap.get(0).unwrap().end();
        let method_sig = extract_method_signature(content, match_end);
        let (path_vars, query_params, body_type, headers) = extract_params(&method_sig);

        endpoints.push(DiscoveredEndpoint {
            group,
            method: method.to_string(),
            route: wire_route,
            name,
            headers,
            query_params,
            body_type,
            body_fields: Vec::new(),
            response_type: None,
            response_fields: Vec::new(),
        });

        // path_vars are already in the route as {{param}} — we don't add them separately
        let _ = path_vars;
    }

    // Match @RequestMapping with method parameter
    let request_mapping_re =
        Regex::new(r#"(?m)@RequestMapping\s*\([^)]*method\s*=\s*(?:RequestMethod\.)?(\w+)[^)]*\)"#)
            .unwrap();
    // Match path from named value/path attribute
    let request_mapping_named_path_re =
        Regex::new(r#"(?m)@RequestMapping\s*\([^)]*(?:value|path)\s*=\s*["']([^"']+)["']"#)
            .unwrap();
    // Match bare positional path: @RequestMapping("/path", method = ...)
    let request_mapping_bare_path_re =
        Regex::new(r#"(?m)@RequestMapping\s*\(\s*["']([^"']+)["']"#).unwrap();

    for cap in request_mapping_re.captures_iter(content) {
        let method = cap[1].to_uppercase();
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            continue;
        }

        let match_start = cap.get(0).unwrap().start();
        let match_end = cap.get(0).unwrap().end();

        // Check this isn't a class-level mapping (followed by "class")
        let remaining = &content[match_end..];
        let scan_window = &remaining[..remaining.len().min(200)];
        if scan_window.contains("class ") || scan_window.contains("object ") {
            continue;
        }

        // Extract path from the same @RequestMapping annotation
        let annotation_text = &content[match_start..match_end];
        let path = request_mapping_named_path_re
            .captures(annotation_text)
            .or_else(|| request_mapping_bare_path_re.captures(annotation_text))
            .map(|c| c[1].to_string())
            .unwrap_or_default();

        let full_route = build_route(class_prefix, &path);
        let wire_route = convert_spring_params(&full_route);
        let name = derive_name(&method, &wire_route);
        let group = group_from_route(&wire_route);

        let method_sig = extract_method_signature(content, match_end);
        let (_path_vars, query_params, body_type, headers) = extract_params(&method_sig);

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

/// Combine class prefix and method path into a full route.
fn build_route(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = normalize_path(path);
    if path.is_empty() {
        if prefix.is_empty() {
            "/".to_string()
        } else {
            prefix.to_string()
        }
    } else {
        format!("{}{}", prefix, path)
    }
}

/// Ensure path starts with `/`.
fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Convert Spring Boot path parameters `{param}` to Wire `{{param}}` syntax.
fn convert_spring_params(route: &str) -> String {
    let re = Regex::new(r"\{(\w+)}").unwrap();
    re.replace_all(route, "{{$1}}").to_string()
}

/// Extract the method signature (up to the opening brace or `=` for Kotlin expression bodies).
fn extract_method_signature(content: &str, start: usize) -> String {
    let remaining = &content[start..];
    // Find end of method signature: opening `{` or `=` for expression bodies
    let end = remaining
        .find('{')
        .or_else(|| remaining.find('='))
        .unwrap_or_else(|| remaining.len().min(500));
    remaining[..end].to_string()
}

/// Extracted parameter info: (path_variables, query_params, body_type, headers)
type ExtractedParams = (
    Vec<String>,
    Vec<(String, String)>,
    Option<String>,
    Vec<(String, String)>,
);

/// Extract parameter annotations from a method signature.
///
/// Returns (path_variables, query_params, body_type, headers).
fn extract_params(sig: &str) -> ExtractedParams {
    let mut path_vars = Vec::new();
    let mut query_params = Vec::new();
    let mut body_type = None;
    let mut headers = Vec::new();

    let mut seen_query = HashSet::new();
    let mut seen_headers = HashSet::new();

    // @PathVariable("name") or @PathVariable String name
    let path_var_re = Regex::new(
        r#"@PathVariable\s*(?:\(\s*(?:value\s*=\s*)?["'](\w+)["']\s*\))?\s*(?:\w+\s+)?(\w+)"#,
    )
    .unwrap();
    for cap in path_var_re.captures_iter(sig) {
        let name = cap
            .get(1)
            .or(cap.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if !name.is_empty() {
            path_vars.push(name);
        }
    }

    // @RequestParam("name") or @RequestParam String name
    let query_re = Regex::new(
        r#"@RequestParam\s*(?:\(\s*(?:value\s*=\s*|name\s*=\s*)?["'](\w+)["'](?:\s*,\s*[^)]*)?(?:\))|(?:\(\s*\))|\s)\s*(?:\w+\s+)?(\w+)"#,
    )
    .unwrap();
    for cap in query_re.captures_iter(sig) {
        let name = cap
            .get(1)
            .or(cap.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if !name.is_empty() && seen_query.insert(name.clone()) {
            query_params.push((name, String::new()));
        }
    }

    // @RequestBody — extract the type name
    // Java: @RequestBody CreateItemDto dto  (type before name)
    // Kotlin: @RequestBody dto: CreateItemDto  (name before type)
    let body_re_kotlin = Regex::new(r#"@RequestBody\s+\w+\s*:\s*(\w+)"#).unwrap();
    let body_re_java = Regex::new(r#"@RequestBody\s+(\w+)"#).unwrap();
    if let Some(cap) = body_re_kotlin.captures(sig) {
        body_type = Some(cap[1].to_string());
    } else if let Some(cap) = body_re_java.captures(sig) {
        body_type = Some(cap[1].to_string());
    }

    // @RequestHeader("name") or @RequestHeader String name
    let header_re = Regex::new(
        r#"@RequestHeader\s*(?:\(\s*(?:value\s*=\s*|name\s*=\s*)?["']([^"']+)["'](?:\s*,\s*[^)]*)?(?:\))|(?:\(\s*\))|\s)\s*(?:\w+\s+)?(\w+)"#,
    )
    .unwrap();
    for cap in header_re.captures_iter(sig) {
        let name = cap
            .get(1)
            .or(cap.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if !name.is_empty() && seen_headers.insert(name.clone()) {
            headers.push((name, String::new()));
        }
    }

    (path_vars, query_params, body_type, headers)
}

/// Derive a human-readable name from HTTP method and route.
/// e.g., GET /api/users/{{id}} → "GetUsersById"
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
            // Extract param name from {{param}}
            let param = part.trim_start_matches("{{").trim_end_matches("}}");
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

/// Derive group from route: /api/users/{{id}} → "users"
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
    fn basic_get_mapping() {
        let content = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping
    public List<User> getAll() {
        return userService.findAll();
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/users");
        assert_eq!(endpoints[0].name, "GetUsers");
        assert_eq!(endpoints[0].group, "users");
    }

    #[test]
    fn post_mapping_with_path() {
        let content = r#"
@RestController
@RequestMapping("/api")
public class ItemController {

    @PostMapping("/items")
    public Item create(@RequestBody ItemDto item) {
        return itemService.save(item);
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "POST");
        assert_eq!(endpoints[0].route, "/api/items");
        assert_eq!(endpoints[0].body_type, Some("ItemDto".to_string()));
    }

    #[test]
    fn path_variable_extraction() {
        let content = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping("/{id}")
    public User getById(@PathVariable Long id) {
        return userService.findById(id);
    }

    @GetMapping("/{userId}/posts/{postId}")
    public Post getUserPost(@PathVariable("userId") Long userId, @PathVariable("postId") Long postId) {
        return postService.findByUserAndId(userId, postId);
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].route, "/api/users/{{id}}");
        assert_eq!(endpoints[0].name, "GetUsersById");
        assert_eq!(endpoints[1].route, "/api/users/{{userId}}/posts/{{postId}}");
        assert_eq!(endpoints[1].name, "GetUsersByUserIdPostsByPostId");
    }

    #[test]
    fn request_param_extraction() {
        let content = r#"
@RestController
public class SearchController {

    @GetMapping("/search")
    public List<Item> search(
            @RequestParam("q") String query,
            @RequestParam String page,
            @RequestParam(value = "limit", defaultValue = "20") int limit) {
        return searchService.search(query, page, limit);
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].query_params.len(), 3);
        assert_eq!(endpoints[0].query_params[0].0, "q");
        assert_eq!(endpoints[0].query_params[1].0, "page");
        assert_eq!(endpoints[0].query_params[2].0, "limit");
    }

    #[test]
    fn request_header_extraction() {
        let content = r#"
@RestController
public class AuthController {

    @GetMapping("/me")
    public User getCurrentUser(@RequestHeader("Authorization") String auth) {
        return userService.getByToken(auth);
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].headers.len(), 1);
        assert_eq!(endpoints[0].headers[0].0, "Authorization");
    }

    #[test]
    fn request_body_type() {
        let content = r#"
@RestController
@RequestMapping("/api")
public class OrderController {

    @PostMapping("/orders")
    public Order createOrder(@RequestBody CreateOrderRequest request) {
        return orderService.create(request);
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints[0].body_type,
            Some("CreateOrderRequest".to_string())
        );
    }

    #[test]
    fn all_http_methods() {
        let content = r#"
@RestController
@RequestMapping("/api/items")
public class ItemController {

    @GetMapping
    public List<Item> list() { return null; }

    @PostMapping
    public Item create(@RequestBody Item item) { return null; }

    @PutMapping("/{id}")
    public Item update(@PathVariable Long id, @RequestBody Item item) { return null; }

    @PatchMapping("/{id}")
    public Item patch(@PathVariable Long id, @RequestBody Item item) { return null; }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable Long id) {}
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 5);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/items");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/api/items");
        assert_eq!(endpoints[2].method, "PUT");
        assert_eq!(endpoints[2].route, "/api/items/{{id}}");
        assert_eq!(endpoints[3].method, "PATCH");
        assert_eq!(endpoints[3].route, "/api/items/{{id}}");
        assert_eq!(endpoints[4].method, "DELETE");
        assert_eq!(endpoints[4].route, "/api/items/{{id}}");
    }

    #[test]
    fn request_mapping_with_method() {
        let content = r#"
@RestController
public class LegacyController {

    @RequestMapping(value = "/legacy", method = RequestMethod.GET)
    public String getLegacy() {
        return "hello";
    }

    @RequestMapping(method = RequestMethod.POST, value = "/legacy")
    public String postLegacy(@RequestBody String body) {
        return body;
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/legacy");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/legacy");
    }

    #[test]
    fn request_mapping_bare_positional_path() {
        let content = r#"
@RestController
public class BareController {

    @RequestMapping("/bare", method = RequestMethod.GET)
    public String getBare() {
        return "bare";
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/bare");
    }

    #[test]
    fn get_mapping_with_path_attribute() {
        let content = r#"
@RestController
public class PathController {

    @GetMapping(path = "/items")
    public List<Item> list() { return null; }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/items");
    }

    #[test]
    fn controller_annotation_not_rest_controller() {
        let content = r#"
@Controller
@RequestMapping("/web")
public class WebController {

    @GetMapping("/home")
    public String home() {
        return "home";
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].route, "/web/home");
    }

    #[test]
    fn no_class_level_request_mapping() {
        let content = r#"
@RestController
public class SimpleController {

    @GetMapping("/health")
    public String health() {
        return "ok";
    }
}
"#;
        let prefix = extract_class_request_mapping(content);
        assert_eq!(prefix, "");
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].route, "/health");
    }

    #[test]
    fn kotlin_controller() {
        let content = r#"
@RestController
@RequestMapping("/api/items")
class ItemController(private val service: ItemService) {

    @GetMapping
    fun listAll(): List<Item> = service.findAll()

    @GetMapping("/{id}")
    fun getById(@PathVariable id: Long): Item = service.findById(id)

    @PostMapping
    fun create(@RequestBody dto: CreateItemDto): Item = service.create(dto)
}
"#;
        let prefix = extract_class_request_mapping(content);
        let endpoints = parse_controller_endpoints(content, &prefix);
        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/items");
        assert_eq!(endpoints[1].method, "GET");
        assert_eq!(endpoints[1].route, "/api/items/{{id}}");
        assert_eq!(endpoints[2].method, "POST");
        assert_eq!(endpoints[2].route, "/api/items");
        assert_eq!(endpoints[2].body_type, Some("CreateItemDto".to_string()));
    }

    #[test]
    fn convert_spring_params_basic() {
        assert_eq!(convert_spring_params("/users/{id}"), "/users/{{id}}");
        assert_eq!(
            convert_spring_params("/users/{userId}/posts/{postId}"),
            "/users/{{userId}}/posts/{{postId}}"
        );
        assert_eq!(convert_spring_params("/users"), "/users");
    }

    #[test]
    fn derive_name_patterns() {
        assert_eq!(derive_name("GET", "/api/users"), "GetUsers");
        assert_eq!(derive_name("GET", "/api/users/{{id}}"), "GetUsersById");
        assert_eq!(derive_name("POST", "/api/items"), "PostItems");
        assert_eq!(
            derive_name("DELETE", "/api/users/{{id}}"),
            "DeleteUsersById"
        );
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
    fn non_controller_file_ignored() {
        let content = r#"
public class UserService {
    public User findById(Long id) {
        return repository.findById(id);
    }
}
"#;
        assert!(!is_controller(content));
    }

    #[test]
    fn controller_annotation_detected() {
        assert!(is_controller("@RestController\npublic class Foo {}"));
        assert!(is_controller("@Controller\npublic class Bar {}"));
        assert!(!is_controller("public class Baz {}"));
    }

    #[test]
    fn integration_scan_java_project() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src/main/java/com/example/controller");
        fs::create_dir_all(&src).unwrap();

        fs::write(
            src.join("UserController.java"),
            r#"
package com.example.controller;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping
    public List<User> getAll() {
        return userService.findAll();
    }

    @GetMapping("/{id}")
    public User getById(@PathVariable Long id) {
        return userService.findById(id);
    }

    @PostMapping
    public User create(@RequestBody CreateUserDto user) {
        return userService.create(user);
    }

    @PutMapping("/{id}")
    public User update(@PathVariable Long id, @RequestBody UpdateUserDto user) {
        return userService.update(id, user);
    }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable Long id) {
        userService.delete(id);
    }
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_springboot(dir.path());
        assert_eq!(files_scanned, 1);
        assert_eq!(endpoints.len(), 5);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/users");
        assert_eq!(endpoints[0].name, "GetUsers");
        assert_eq!(endpoints[0].group, "users");

        assert_eq!(endpoints[1].method, "GET");
        assert_eq!(endpoints[1].route, "/api/users/{{id}}");

        assert_eq!(endpoints[2].method, "POST");
        assert_eq!(endpoints[2].route, "/api/users");
        assert_eq!(endpoints[2].body_type, Some("CreateUserDto".to_string()));

        assert_eq!(endpoints[3].method, "PUT");
        assert_eq!(endpoints[3].route, "/api/users/{{id}}");
        assert_eq!(endpoints[3].body_type, Some("UpdateUserDto".to_string()));

        assert_eq!(endpoints[4].method, "DELETE");
        assert_eq!(endpoints[4].route, "/api/users/{{id}}");
    }

    #[test]
    fn integration_skip_build_and_target() {
        let dir = TempDir::new().unwrap();

        // Put a controller in target/ — should be skipped
        let target = dir.path().join("target/classes/com/example");
        fs::create_dir_all(&target).unwrap();
        fs::write(
            target.join("FakeController.java"),
            r#"
@RestController
public class FakeController {
    @GetMapping("/fake")
    public String fake() { return "fake"; }
}
"#,
        )
        .unwrap();

        // Put a controller in build/ — should be skipped
        let build = dir.path().join("build/classes/com/example");
        fs::create_dir_all(&build).unwrap();
        fs::write(
            build.join("BuildController.java"),
            r#"
@RestController
public class BuildController {
    @GetMapping("/build")
    public String build() { return "build"; }
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_springboot(dir.path());
        assert_eq!(files_scanned, 0);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn integration_kotlin_file() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src/main/kotlin/com/example");
        fs::create_dir_all(&src).unwrap();

        fs::write(
            src.join("ItemController.kt"),
            r#"
package com.example

import org.springframework.web.bind.annotation.*

@RestController
@RequestMapping("/api/items")
class ItemController(private val service: ItemService) {

    @GetMapping
    fun listAll(): List<Item> = service.findAll()

    @PostMapping
    fun create(@RequestBody dto: CreateItemDto): Item = service.create(dto)
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_springboot(dir.path());
        assert_eq!(files_scanned, 1);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/items");
        assert_eq!(endpoints[1].method, "POST");
        assert_eq!(endpoints[1].route, "/api/items");
        assert_eq!(endpoints[1].body_type, Some("CreateItemDto".to_string()));
    }

    #[test]
    fn skip_test_directory() {
        let dir = TempDir::new().unwrap();
        let test_dir = dir.path().join("src/test/java/com/example");
        fs::create_dir_all(&test_dir).unwrap();

        fs::write(
            test_dir.join("TestController.java"),
            r#"
@RestController
public class TestController {
    @GetMapping("/test")
    public String test() { return "test"; }
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_springboot(dir.path());
        assert_eq!(files_scanned, 0);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn build_route_combines_prefix_and_path() {
        assert_eq!(build_route("/api", "/users"), "/api/users");
        assert_eq!(build_route("/api/", "/users"), "/api/users");
        assert_eq!(build_route("/api", ""), "/api");
        assert_eq!(build_route("", "/users"), "/users");
        assert_eq!(build_route("", ""), "/");
    }

    #[test]
    fn normalize_path_adds_slash() {
        assert_eq!(normalize_path("users"), "/users");
        assert_eq!(normalize_path("/users"), "/users");
        assert_eq!(normalize_path(""), "");
        assert_eq!(normalize_path("  "), "");
    }
}
