use super::types::DiscoveredEndpoint;
use regex::Regex;
use std::path::Path;

/// Scan an ASP.NET project for HTTP endpoints.
/// Detects controller-based APIs and minimal APIs.
pub fn scan_aspnet(project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    let mut endpoints = Vec::new();
    let mut files_scanned = 0;

    let cs_files = collect_cs_files(project_dir);
    for file_path in &cs_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        files_scanned += 1;

        // Try controller-based parsing
        let controller_endpoints = parse_controllers(&content);
        endpoints.extend(controller_endpoints);

        // Try minimal API parsing
        let minimal_endpoints = parse_minimal_apis(&content);
        endpoints.extend(minimal_endpoints);
    }

    // Second pass: resolve body fields from DTO class definitions
    // Collect all source content for class lookup
    let all_sources: Vec<String> = cs_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    let combined_source = all_sources.join("\n");

    for endpoint in &mut endpoints {
        if let Some(ref type_name) = endpoint.body_type {
            endpoint.body_fields = find_class_properties(&combined_source, type_name);
        }
    }

    (endpoints, files_scanned)
}

/// Find public properties of a C# class by name in source code.
/// Extracts auto-properties like: public string Name { get; set; }
fn find_class_properties(source: &str, class_name: &str) -> Vec<(String, String)> {
    // Find the class declaration
    let class_pattern = format!(
        r"(?s)(?:public\s+)?class\s+{}\b[^{{]*\{{",
        regex::escape(class_name)
    );
    let class_re = match Regex::new(&class_pattern) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let class_match = match class_re.find(source) {
        Some(m) => m,
        None => return Vec::new(),
    };

    // Extract the class body using balanced braces
    let start = class_match.end(); // position after the opening {
    let bytes = source.as_bytes();
    let mut depth = 1;
    let mut pos = start;
    while pos < bytes.len() && depth > 0 {
        match bytes[pos] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            pos += 1;
        }
    }
    let class_body = &source[start..pos];

    // Match public properties: public Type Name { get; set; }
    // Handles: nullable types (string?), generic types (List<string>), required keyword
    let prop_re =
        Regex::new(r"(?m)public\s+(?:required\s+)?(\w+(?:\?|<[^>]+>)?)\s+(\w+)\s*\{\s*get;")
            .unwrap();

    let mut properties = Vec::new();
    for cap in prop_re.captures_iter(class_body) {
        let type_name = cap[1].to_string();
        let prop_name = cap[2].to_string();
        properties.push((prop_name, type_name));
    }

    properties
}

/// Collect all .cs files in the project, skipping irrelevant directories.
fn collect_cs_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_cs_files_recursive(dir, &mut files, 10);
    files
}

fn collect_cs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>, depth: u32) {
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
                "bin" | "obj" | "node_modules" | ".git" | "target" | ".wire" | "dist"
            ) {
                continue;
            }
            collect_cs_files_recursive(&path, files, depth - 1);
        } else if path.extension().is_some_and(|e| e == "cs") {
            files.push(path);
        }
    }
}

/// Parse controller-based ASP.NET endpoints from a .cs file's content.
fn parse_controllers(content: &str) -> Vec<DiscoveredEndpoint> {
    let mut endpoints = Vec::new();

    // Match class declarations that look like controllers
    // Pattern: optional [Route("...")] then class SomethingController
    let class_re =
        Regex::new(r#"(?s)\[Route\(\s*"([^"]+)"\s*\)\].*?class\s+(\w+Controller)\b"#).unwrap();

    // Also match controllers without [Route] attribute
    let class_no_route_re = Regex::new(r#"class\s+(\w+Controller)\b"#).unwrap();

    // Find all controller classes with [Route]
    let mut controller_regions: Vec<(String, String, usize)> = Vec::new(); // (route, name, start_pos)

    for cap in class_re.captures_iter(content) {
        let route_template = cap[1].to_string();
        let class_name = cap[2].to_string();
        let start = cap.get(0).unwrap().start();
        controller_regions.push((route_template, class_name, start));
    }

    // If no [Route] controllers found, try without route attribute
    if controller_regions.is_empty() {
        for cap in class_no_route_re.captures_iter(content) {
            let class_name = cap[1].to_string();
            let start = cap.get(0).unwrap().start();
            controller_regions.push((String::new(), class_name, start));
        }
    }

    // Pre-compile regexes outside the loop
    let http_attr_re =
        Regex::new(r#"\[Http(Get|Post|Put|Patch|Delete)(?:\(\s*"([^"]*)"\s*\))?\]"#).unwrap();
    let method_sig_re =
        Regex::new(r#"(?s)(?:\[.*?\]\s*)*(?:public\s+)?(?:\w+(?:<[^>]+>)?)\s+(\w+)\s*\("#).unwrap();

    for (route_template, class_name, class_start) in &controller_regions {
        let controller_short = class_name
            .strip_suffix("Controller")
            .unwrap_or(class_name)
            .to_lowercase();

        let base_route = route_template
            .replace("[controller]", &controller_short)
            .replace("[action]", "");

        // Find the class body region (approximate: from class declaration to the end or next class)
        let class_content = &content[*class_start..];

        for attr_cap in http_attr_re.captures_iter(class_content) {
            let http_method = attr_cap[1].to_uppercase();
            let method_route = attr_cap.get(2).map(|m| m.as_str()).unwrap_or("");

            // Find the method signature after this attribute
            let attr_end = attr_cap.get(0).unwrap().end();
            let after_attr = &class_content[attr_end..];
            let sig_cap = match method_sig_re.captures(after_attr) {
                Some(c) => c,
                None => continue,
            };
            let method_name = &sig_cap[1];

            // Find balanced closing paren for the parameter list
            let paren_start = attr_end + sig_cap.get(0).unwrap().end();
            let params_str = match extract_balanced_parens(class_content, paren_start) {
                Some(s) => s,
                None => continue,
            };

            // Build full route
            let full_route = build_route(&base_route, method_route);
            let wire_route = convert_route_params(&full_route);

            // Extract parameter metadata
            let (headers, query_params, body_type) = parse_params(&params_str);

            endpoints.push(DiscoveredEndpoint {
                method: http_method,
                route: wire_route,
                name: method_name.to_string(),
                headers,
                query_params,
                body_type,
                body_fields: Vec::new(), // populated later via resolve_body_fields
            });
        }
    }

    endpoints
}

/// Parse minimal API patterns: app.MapGet("/route", ...), app.MapPost("/route", ...), etc.
fn parse_minimal_apis(content: &str) -> Vec<DiscoveredEndpoint> {
    let mut endpoints = Vec::new();

    let map_re = Regex::new(
        r#"(?m)\.Map(Get|Post|Put|Patch|Delete)\(\s*"([^"]+)"(?:\s*,\s*\(?([^)]*)\)?)?"#,
    )
    .unwrap();

    for cap in map_re.captures_iter(content) {
        let http_method = cap[1].to_uppercase();
        let route = &cap[2];
        let params_str = cap.get(3).map(|m| m.as_str()).unwrap_or("");

        let wire_route = convert_route_params(route);

        // Derive name from route: /api/users/{id} → "GetUsersById"
        let name = derive_name_from_route(&http_method, route);

        let (headers, query_params, body_type) = parse_minimal_api_params(params_str);

        endpoints.push(DiscoveredEndpoint {
            method: http_method,
            route: wire_route,
            name,
            headers,
            query_params,
            body_type,
            body_fields: Vec::new(), // populated later via resolve_body_fields
        });
    }

    endpoints
}

/// Extract content between balanced parentheses starting at `start` position.
/// `start` should point to the character immediately after the opening `(`.
fn extract_balanced_parens(content: &str, start: usize) -> Option<String> {
    let bytes = content.as_bytes();
    let mut depth = 1;
    let mut pos = start;
    while pos < bytes.len() && depth > 0 {
        match bytes[pos] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            pos += 1;
        }
    }
    if depth == 0 {
        Some(content[start..pos].to_string())
    } else {
        None
    }
}

/// Combine base route and method route into a full path.
fn build_route(base: &str, method_route: &str) -> String {
    let base = base.trim_matches('/');
    let method_route = method_route.trim_matches('/');

    if base.is_empty() && method_route.is_empty() {
        return "/".to_string();
    }
    if base.is_empty() {
        return format!("/{method_route}");
    }
    if method_route.is_empty() {
        return format!("/{base}");
    }
    format!("/{base}/{method_route}")
}

/// Convert ASP.NET route parameters {param} or {param:constraint} to Wire {{param}} syntax.
fn convert_route_params(route: &str) -> String {
    let re = Regex::new(r"\{(\w+)(?::[^}]*)?\}").unwrap();
    re.replace_all(route, "{{$1}}").to_string()
}

/// Extracted parameter metadata: (headers, query_params, body_type)
type ParamMeta = (Vec<(String, String)>, Vec<(String, String)>, Option<String>);

/// Parse controller method parameters for [FromBody], [FromHeader], [FromQuery].
fn parse_params(params_str: &str) -> ParamMeta {
    let mut headers = Vec::new();
    let mut query_params = Vec::new();
    let mut body_type = None;

    // Match [FromBody] TypeName paramName
    let from_body_re = Regex::new(r#"\[FromBody\]\s*(\w+)\s+(\w+)"#).unwrap();
    if let Some(cap) = from_body_re.captures(params_str) {
        body_type = Some(cap[1].to_string());
    }

    // Match [FromHeader] or [FromHeader(Name = "X-Custom")]
    let from_header_re =
        Regex::new(r#"\[FromHeader(?:\(Name\s*=\s*"([^"]+)"\))?\]\s*\w+\s+(\w+)"#).unwrap();
    for cap in from_header_re.captures_iter(params_str) {
        let header_name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| cap[2].to_string());
        headers.push((header_name, String::new()));
    }

    // Match [FromQuery] or [FromQuery(Name = "filter")]
    let from_query_re =
        Regex::new(r#"\[FromQuery(?:\(Name\s*=\s*"([^"]+)"\))?\]\s*\w+\s+(\w+)"#).unwrap();
    for cap in from_query_re.captures_iter(params_str) {
        let param_name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| cap[2].to_string());
        query_params.push((param_name, String::new()));
    }

    (headers, query_params, body_type)
}

/// Parse minimal API handler parameters for [FromBody], [FromHeader], [FromQuery].
fn parse_minimal_api_params(params_str: &str) -> ParamMeta {
    // Minimal APIs use the same attributes as controllers
    parse_params(params_str)
}

/// Derive a human-readable name from a route pattern.
/// e.g., GET /api/users/{id} → "GetUsersById"
fn derive_name_from_route(method: &str, route: &str) -> String {
    let parts: Vec<&str> = route
        .split('/')
        .filter(|s| !s.is_empty() && *s != "api")
        .collect();

    let mut name = method
        .chars()
        .next()
        .unwrap_or('G')
        .to_uppercase()
        .to_string()
        + &method[1..].to_lowercase();

    for part in parts {
        if part.starts_with('{') && part.ends_with('}') {
            let param = &part[1..part.len() - 1];
            name.push_str("By");
            name.push_str(&capitalize(param));
        } else {
            name.push_str(&capitalize(part));
        }
    }

    if name == method {
        // Fallback if route produced nothing useful
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_basic_controller() {
        let code = r#"
[ApiController]
[Route("api/[controller]")]
public class UsersController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll()
    {
        return Ok();
    }

    [HttpGet("{id}")]
    public IActionResult GetById(int id)
    {
        return Ok();
    }

    [HttpPost]
    public IActionResult Create([FromBody] CreateUserDto dto)
    {
        return Ok();
    }
}
"#;
        let endpoints = parse_controllers(code);
        assert_eq!(endpoints.len(), 3);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/users");
        assert_eq!(endpoints[0].name, "GetAll");

        assert_eq!(endpoints[1].method, "GET");
        assert_eq!(endpoints[1].route, "/api/users/{{id}}");
        assert_eq!(endpoints[1].name, "GetById");

        assert_eq!(endpoints[2].method, "POST");
        assert_eq!(endpoints[2].route, "/api/users");
        assert_eq!(endpoints[2].name, "Create");
        assert_eq!(endpoints[2].body_type, Some("CreateUserDto".to_string()));
    }

    #[test]
    fn parse_controller_with_all_methods() {
        let code = r#"
[Route("api/[controller]")]
public class ProductsController : ControllerBase
{
    [HttpGet]
    public IActionResult List() { return Ok(); }

    [HttpPost]
    public IActionResult Create([FromBody] ProductDto dto) { return Ok(); }

    [HttpPut("{id}")]
    public IActionResult Update(int id, [FromBody] ProductDto dto) { return Ok(); }

    [HttpPatch("{id}")]
    public IActionResult Patch(int id) { return Ok(); }

    [HttpDelete("{id}")]
    public IActionResult Delete(int id) { return Ok(); }
}
"#;
        let endpoints = parse_controllers(code);
        assert_eq!(endpoints.len(), 5);

        let methods: Vec<&str> = endpoints.iter().map(|e| e.method.as_str()).collect();
        assert_eq!(methods, vec!["GET", "POST", "PUT", "PATCH", "DELETE"]);

        assert_eq!(endpoints[2].route, "/api/products/{{id}}");
        assert_eq!(endpoints[2].body_type, Some("ProductDto".to_string()));
    }

    #[test]
    fn parse_controller_with_from_header_and_query() {
        let code = r#"
[Route("api/[controller]")]
public class SearchController : ControllerBase
{
    [HttpGet]
    public IActionResult Search(
        [FromQuery] string q,
        [FromQuery(Name = "page_size")] int pageSize,
        [FromHeader(Name = "X-Request-Id")] string requestId)
    {
        return Ok();
    }
}
"#;
        let endpoints = parse_controllers(code);
        assert_eq!(endpoints.len(), 1);

        let ep = &endpoints[0];
        assert_eq!(ep.query_params.len(), 2);
        assert_eq!(ep.query_params[0].0, "q");
        assert_eq!(ep.query_params[1].0, "page_size");

        assert_eq!(ep.headers.len(), 1);
        assert_eq!(ep.headers[0].0, "X-Request-Id");
    }

    #[test]
    fn parse_minimal_api_endpoints() {
        let code = r#"
var app = builder.Build();

app.MapGet("/api/users", () => Results.Ok());
app.MapGet("/api/users/{id}", (int id) => Results.Ok());
app.MapPost("/api/users", ([FromBody] CreateUserDto dto) => Results.Ok());
app.MapDelete("/api/users/{id}", (int id) => Results.Ok());
"#;
        let endpoints = parse_minimal_apis(code);
        assert_eq!(endpoints.len(), 4);

        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/api/users");

        assert_eq!(endpoints[1].method, "GET");
        assert_eq!(endpoints[1].route, "/api/users/{{id}}");

        assert_eq!(endpoints[2].method, "POST");
        assert_eq!(endpoints[2].route, "/api/users");
        assert_eq!(endpoints[2].body_type, Some("CreateUserDto".to_string()));

        assert_eq!(endpoints[3].method, "DELETE");
        assert_eq!(endpoints[3].route, "/api/users/{{id}}");
    }

    #[test]
    fn convert_route_params_replaces_braces() {
        assert_eq!(convert_route_params("/api/users/{id}"), "/api/users/{{id}}");
        assert_eq!(
            convert_route_params("/api/{orgId}/users/{userId}"),
            "/api/{{orgId}}/users/{{userId}}"
        );
        assert_eq!(convert_route_params("/api/health"), "/api/health");
        // Type-constrained parameters
        assert_eq!(
            convert_route_params("/api/tours/{id:guid}"),
            "/api/tours/{{id}}"
        );
        assert_eq!(
            convert_route_params("/api/users/{id:int}/posts/{postId:guid}"),
            "/api/users/{{id}}/posts/{{postId}}"
        );
    }

    #[test]
    fn build_route_combines_base_and_method() {
        assert_eq!(build_route("api/users", ""), "/api/users");
        assert_eq!(build_route("api/users", "{id}"), "/api/users/{id}");
        assert_eq!(build_route("", "health"), "/health");
        assert_eq!(build_route("", ""), "/");
    }

    #[test]
    fn derive_name_from_route_generates_readable_names() {
        assert_eq!(derive_name_from_route("GET", "/api/users"), "GetUsers");
        assert_eq!(
            derive_name_from_route("GET", "/api/users/{id}"),
            "GetUsersById"
        );
        assert_eq!(derive_name_from_route("POST", "/api/users"), "PostUsers");
        assert_eq!(
            derive_name_from_route("DELETE", "/api/users/{id}"),
            "DeleteUsersById"
        );
    }

    #[test]
    fn scan_aspnet_with_real_files() {
        let dir = TempDir::new().unwrap();
        let controllers_dir = dir.path().join("Controllers");
        fs::create_dir_all(&controllers_dir).unwrap();

        fs::write(
            controllers_dir.join("WeatherController.cs"),
            r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
[Route("api/[controller]")]
public class WeatherController : ControllerBase
{
    [HttpGet]
    public IActionResult GetForecast()
    {
        return Ok();
    }

    [HttpGet("{city}")]
    public IActionResult GetByCity(string city)
    {
        return Ok();
    }
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_aspnet(dir.path());
        assert_eq!(files_scanned, 1);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].route, "/api/weather");
        assert_eq!(endpoints[0].name, "GetForecast");
        assert_eq!(endpoints[1].route, "/api/weather/{{city}}");
    }

    #[test]
    fn scan_aspnet_skips_bin_obj() {
        let dir = TempDir::new().unwrap();

        // File in bin/ should be skipped
        let bin_dir = dir.path().join("bin/Debug/net8.0");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(
            bin_dir.join("Compiled.cs"),
            r#"
[Route("api/[controller]")]
public class FakeController : ControllerBase
{
    [HttpGet]
    public IActionResult Get() { return Ok(); }
}
"#,
        )
        .unwrap();

        let (endpoints, files_scanned) = scan_aspnet(dir.path());
        assert_eq!(files_scanned, 0);
        assert_eq!(endpoints.len(), 0);
    }

    #[test]
    fn parse_controller_with_multiple_route_params() {
        let code = r#"
[Route("api/[controller]")]
public class OrdersController : ControllerBase
{
    [HttpGet("{customerId}/orders/{orderId}")]
    public IActionResult GetOrder(int customerId, int orderId)
    {
        return Ok();
    }
}
"#;
        let endpoints = parse_controllers(code);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints[0].route,
            "/api/orders/{{customerId}}/orders/{{orderId}}"
        );
    }

    #[test]
    fn no_endpoints_in_non_controller_code() {
        let code = r#"
public class UserService
{
    public User GetUser(int id) { return null; }
    public void CreateUser(CreateUserDto dto) { }
}
"#;
        let endpoints = parse_controllers(code);
        assert!(endpoints.is_empty());
    }

    #[test]
    fn minimal_api_with_no_params() {
        let code = r#"
app.MapGet("/health", () => Results.Ok("healthy"));
"#;
        let endpoints = parse_minimal_apis(code);
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].method, "GET");
        assert_eq!(endpoints[0].route, "/health");
        assert_eq!(endpoints[0].name, "GetHealth");
    }

    #[test]
    fn find_class_properties_extracts_auto_properties() {
        let source = r#"
public class CreateTourDto
{
    public string Name { get; set; }
    public double Latitude { get; set; }
    public double Longitude { get; set; }
    public Guid BreweryId { get; set; }
    public int? Rating { get; set; }
}
"#;
        let props = find_class_properties(source, "CreateTourDto");
        assert_eq!(props.len(), 5);
        assert_eq!(props[0], ("Name".to_string(), "string".to_string()));
        assert_eq!(props[1], ("Latitude".to_string(), "double".to_string()));
        assert_eq!(props[3], ("BreweryId".to_string(), "Guid".to_string()));
        assert_eq!(props[4], ("Rating".to_string(), "int?".to_string()));
    }

    #[test]
    fn find_class_properties_handles_required_keyword() {
        let source = r#"
public class UserDto
{
    public required string Email { get; set; }
    public string? DisplayName { get; set; }
}
"#;
        let props = find_class_properties(source, "UserDto");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0], ("Email".to_string(), "string".to_string()));
        assert_eq!(props[1], ("DisplayName".to_string(), "string?".to_string()));
    }

    #[test]
    fn find_class_properties_not_found_returns_empty() {
        let source = "public class Unrelated { public string Foo { get; set; } }";
        let props = find_class_properties(source, "NonExistentDto");
        assert!(props.is_empty());
    }

    #[test]
    fn scan_aspnet_resolves_body_fields_from_dto() {
        let dir = TempDir::new().unwrap();
        let controllers_dir = dir.path().join("Controllers");
        let models_dir = dir.path().join("Models");
        fs::create_dir_all(&controllers_dir).unwrap();
        fs::create_dir_all(&models_dir).unwrap();

        fs::write(
            controllers_dir.join("ToursController.cs"),
            r#"
[ApiController]
[Route("api/[controller]")]
public class ToursController : ControllerBase
{
    [HttpPost]
    public IActionResult Create([FromBody] CreateTourDto dto)
    {
        return Ok();
    }
}
"#,
        )
        .unwrap();

        fs::write(
            models_dir.join("CreateTourDto.cs"),
            r#"
public class CreateTourDto
{
    public string Name { get; set; }
    public double Latitude { get; set; }
    public Guid BreweryId { get; set; }
}
"#,
        )
        .unwrap();

        let (endpoints, _) = scan_aspnet(dir.path());
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].body_type, Some("CreateTourDto".to_string()));
        assert_eq!(endpoints[0].body_fields.len(), 3);
        assert_eq!(endpoints[0].body_fields[0].0, "Name");
        assert_eq!(endpoints[0].body_fields[1].0, "Latitude");
        assert_eq!(endpoints[0].body_fields[2].0, "BreweryId");
    }
}
