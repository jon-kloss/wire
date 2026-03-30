mod aspnet;
mod detect;
pub mod envdiscover;
mod express;
pub mod types;

use crate::collection::{
    load_collection, Body, BodyType, Environment, LoadedCollection, WireCollection, WireRequest,
};
use crate::error::WireError;
use std::path::Path;
use types::{DiscoveredEndpoint, Framework, ScanResult};

pub use detect::detect_framework;
pub use types::ScanResult as ScanResultType;

/// Scan a project directory and create a Wire collection from discovered endpoints.
///
/// Returns the scan result metadata and the loaded collection (if any endpoints found).
pub fn scan_and_create_collection(
    project_dir: &Path,
    output_dir: &Path,
) -> Result<(ScanResult, Option<LoadedCollection>), WireError> {
    let scan = scan_project(project_dir)?;

    if scan.endpoints.is_empty() {
        return Ok((scan, None));
    }

    // Derive collection name from the project directory name
    let project_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Scanned API".to_string());

    let wire_dir = output_dir.join(".wire");
    // Clear old requests on re-scan so stale endpoints don't persist
    let requests_dir = wire_dir.join("requests");
    if requests_dir.is_dir() {
        std::fs::remove_dir_all(&requests_dir)?;
    }
    std::fs::create_dir_all(wire_dir.join("envs"))?;
    std::fs::create_dir_all(&requests_dir)?;

    // Discover environments from project config files
    let discovered_envs = envdiscover::discover_environments(project_dir, &scan.framework);

    // Determine active environment (first discovered, or "dev" fallback)
    let active_env = discovered_envs
        .first()
        .map(|e| e.filename.clone())
        .unwrap_or_else(|| "dev".to_string());

    // Write collection metadata
    let metadata = WireCollection {
        name: project_name,
        version: 1,
        active_env: Some(active_env),
        default_template: None,
        default_templates: Vec::new(),
        source_dir: Some(project_dir.to_string_lossy().to_string()),
    };
    let metadata_yaml = serde_yaml::to_string(&metadata)?;
    std::fs::write(wire_dir.join("wire.yaml"), metadata_yaml)?;

    // Collect all unique route parameters from endpoints
    let route_param_re = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();
    let mut route_params = std::collections::BTreeSet::new();
    for endpoint in &scan.endpoints {
        for cap in route_param_re.captures_iter(&endpoint.route) {
            route_params.insert(cap[1].to_string());
        }
    }

    // Write discovered environments, or fall back to default dev.yaml
    // Inject route params as empty variables into every environment
    if discovered_envs.is_empty() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("schema".to_string(), "http".to_string());
        vars.insert("baseUrl".to_string(), "localhost:3000".to_string());
        for param in &route_params {
            vars.entry(param.clone()).or_insert_with(String::new);
        }
        let dev_env = Environment {
            name: "Development".to_string(),
            variables: vars,
        };
        let dev_yaml = serde_yaml::to_string(&dev_env)?;
        std::fs::write(wire_dir.join("envs/dev.yaml"), dev_yaml)?;
    } else {
        for env in &discovered_envs {
            let mut variables = env.variables.clone();
            for param in &route_params {
                variables.entry(param.clone()).or_insert_with(String::new);
            }
            let wire_env = Environment {
                name: env.name.clone(),
                variables,
            };
            let yaml = serde_yaml::to_string(&wire_env)?;
            std::fs::write(
                wire_dir.join("envs").join(format!("{}.yaml", env.filename)),
                yaml,
            )?;
        }
    }

    // Write each endpoint as a .wire.yaml request file, grouped by controller/router
    for endpoint in &scan.endpoints {
        let request = endpoint_to_request(endpoint);
        let filename = slugify(&endpoint.name) + ".wire.yaml";
        let group_dir = wire_dir.join("requests").join(&endpoint.group);
        std::fs::create_dir_all(&group_dir)?;
        let file_path = group_dir.join(&filename);

        // Avoid overwriting duplicates — append a number
        let final_path = unique_path(&file_path);
        let yaml = serde_yaml::to_string(&request)?;
        std::fs::write(&final_path, yaml)?;
    }

    let collection = load_collection(&wire_dir)?;
    Ok((scan, Some(collection)))
}

/// Convert a DiscoveredEndpoint to a WireRequest.
pub fn endpoint_to_request(endpoint: &DiscoveredEndpoint) -> WireRequest {
    let mut headers = std::collections::HashMap::new();
    for (name, _) in &endpoint.headers {
        headers.insert(name.clone(), String::new());
    }

    let mut params = std::collections::HashMap::new();
    for (name, _) in &endpoint.query_params {
        params.insert(name.clone(), String::new());
    }

    // Prefix route with base URL template
    let url = format!("{{{{schema}}}}://{{{{baseUrl}}}}{}", endpoint.route);

    // Build body from discovered fields
    let body = if !endpoint.body_fields.is_empty() {
        let mut map = serde_json::Map::new();
        for (field_name, type_hint) in &endpoint.body_fields {
            let camel = to_camel_case(field_name);
            map.insert(camel, csharp_type_default(type_hint));
        }
        Some(Body {
            body_type: BodyType::Json,
            content: serde_json::Value::Object(map),
        })
    } else {
        None
    };

    WireRequest {
        name: endpoint.name.clone(),
        method: endpoint.method.clone(),
        url,
        headers,
        params,
        body,
        extends: None,
        tests: Vec::new(),
        response_schema: endpoint.response_fields.clone(),
        chain: Vec::new(),
        snapshot: None,
    }
}

/// Convert a PascalCase property name to camelCase for JSON.
fn to_camel_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().to_string() + chars.as_str(),
    }
}

/// Map a C# type name to a sensible JSON default value.
fn csharp_type_default(type_hint: &str) -> serde_json::Value {
    let normalized = type_hint.trim_end_matches('?').to_lowercase();
    if normalized.starts_with("list<") || normalized.starts_with("ienumerable<") {
        return serde_json::Value::Array(Vec::new());
    }
    match normalized.as_str() {
        "int" | "long" | "short" | "byte" | "float" | "double" | "decimal" => {
            serde_json::Value::Number(serde_json::Number::from(0))
        }
        "bool" | "boolean" => serde_json::Value::Bool(false),
        "datetime" | "datetimeoffset" => {
            serde_json::Value::String("2025-01-01T00:00:00Z".to_string())
        }
        _ => serde_json::Value::String(String::new()),
    }
}

/// Convert a name to a filename-safe slug.
fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// If `path` already exists, append -2, -3, etc. until unique.
fn unique_path(path: &Path) -> std::path::PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(path);
    for i in 2..100 {
        let candidate = parent.join(format!("{stem}-{i}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// Scan a project directory for HTTP endpoints.
///
/// Auto-detects the framework type and runs the appropriate parser.
/// Returns a ScanResult with discovered endpoints and metadata.
pub fn scan_project(project_dir: &Path) -> Result<ScanResult, WireError> {
    if !project_dir.is_dir() {
        return Err(WireError::Other(format!(
            "Not a directory: {}",
            project_dir.display()
        )));
    }

    let framework = detect_framework(project_dir);

    let (endpoints, files_scanned) = match framework {
        Framework::AspNet => aspnet::scan_aspnet(project_dir),
        Framework::Express => express::scan_express(project_dir),
        Framework::Unknown => {
            // Try all parsers when framework is unknown
            let (mut endpoints, mut files) = aspnet::scan_aspnet(project_dir);
            let (express_endpoints, express_files) = express::scan_express(project_dir);
            endpoints.extend(express_endpoints);
            files += express_files;
            (endpoints, files)
        }
    };

    Ok(ScanResult {
        framework,
        endpoints,
        files_scanned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn scan_project_invalid_path() {
        let result = scan_project(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Not a directory"));
    }

    #[test]
    fn scan_project_empty_dir_returns_no_endpoints() {
        let dir = TempDir::new().unwrap();
        let result = scan_project(dir.path()).unwrap();
        assert_eq!(result.framework, Framework::Unknown);
        assert!(result.endpoints.is_empty());
        assert_eq!(result.files_scanned, 0);
    }

    #[test]
    fn scan_project_detects_aspnet() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();

        let result = scan_project(dir.path()).unwrap();
        assert_eq!(result.framework, Framework::AspNet);
        // Placeholder parsers return empty — will be populated in later tasks
        assert!(result.endpoints.is_empty());
    }

    #[test]
    fn scan_project_detects_express() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();

        let result = scan_project(dir.path()).unwrap();
        assert_eq!(result.framework, Framework::Express);
        assert!(result.endpoints.is_empty());
    }

    #[test]
    fn scan_project_unknown_framework_runs_all_parsers() {
        let dir = TempDir::new().unwrap();
        // No framework markers — should still succeed with 0 endpoints
        let result = scan_project(dir.path()).unwrap();
        assert_eq!(result.framework, Framework::Unknown);
        assert!(result.endpoints.is_empty());
    }

    #[test]
    fn scan_and_create_collection_with_express() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        // Create an Express project
        fs::write(
            project_dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();
        fs::create_dir_all(project_dir.path().join("routes")).unwrap();
        fs::write(
            project_dir.path().join("routes/users.js"),
            r#"
const router = require('express').Router();
router.get('/users', (req, res) => { res.json([]); });
router.post('/users', (req, res) => { res.json(req.body); });
router.get('/users/:id', (req, res) => { res.json({}); });
module.exports = router;
"#,
        )
        .unwrap();

        let (scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();

        assert_eq!(scan.framework, Framework::Express);
        assert_eq!(scan.endpoints.len(), 3);

        let collection = collection.expect("should have created collection");
        assert_eq!(collection.requests.len(), 3);

        // Verify .wire directory was created
        assert!(output_dir.path().join(".wire/wire.yaml").exists());
        assert!(output_dir.path().join(".wire/requests").is_dir());

        // Verify dev.yaml environment was created
        assert!(output_dir.path().join(".wire/envs/dev.yaml").exists());
        assert_eq!(collection.environments.len(), 1);
        assert!(collection.environments.contains_key("dev"));
        let dev = &collection.environments["dev"];
        assert_eq!(dev.variables["schema"], "http");
        assert_eq!(dev.variables["baseUrl"], "localhost:3000");

        // Verify URLs have base URL template prefix
        let urls: Vec<&str> = collection
            .requests
            .iter()
            .map(|(_, r)| r.url.as_str())
            .collect();
        assert!(urls.contains(&"{{schema}}://{{baseUrl}}/users"));
        assert!(urls.contains(&"{{schema}}://{{baseUrl}}/users/{{id}}"));

        // Verify active_env is set to dev
        assert_eq!(collection.metadata.active_env, Some("dev".to_string()));
    }

    #[test]
    fn scan_and_create_collection_with_aspnet() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        fs::write(
            project_dir.path().join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();
        fs::create_dir_all(project_dir.path().join("Controllers")).unwrap();
        fs::write(
            project_dir.path().join("Controllers/ItemsController.cs"),
            r#"
[ApiController]
[Route("api/[controller]")]
public class ItemsController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll() { return Ok(); }

    [HttpGet("{id}")]
    public IActionResult GetById(int id) { return Ok(); }
}
"#,
        )
        .unwrap();

        let (scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();

        assert_eq!(scan.framework, Framework::AspNet);
        assert_eq!(scan.endpoints.len(), 2);

        let collection = collection.expect("should have created collection");
        assert_eq!(collection.requests.len(), 2);

        // Verify URLs have base URL template prefix with route params
        let urls: Vec<&str> = collection
            .requests
            .iter()
            .map(|(_, r)| r.url.as_str())
            .collect();
        assert!(urls.contains(&"{{schema}}://{{baseUrl}}/api/items"));
        assert!(urls.contains(&"{{schema}}://{{baseUrl}}/api/items/{{id}}"));

        // Verify dev.yaml created
        assert!(output_dir.path().join(".wire/envs/dev.yaml").exists());
    }

    #[test]
    fn scan_and_create_collection_empty_project() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        let (scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();

        assert!(scan.endpoints.is_empty());
        assert!(collection.is_none());
        // No .wire directory should be created
        assert!(!output_dir.path().join(".wire").exists());
    }

    #[test]
    fn slugify_produces_safe_filenames() {
        assert_eq!(slugify("GetUsers"), "getusers");
        assert_eq!(slugify("GetUsersById"), "getusersbyid");
        assert_eq!(slugify("POST /api/users"), "post--api-users");
    }

    #[test]
    fn scan_and_create_collection_discovers_aspnet_envs() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        // ASP.NET project with appsettings + controller
        fs::write(
            project_dir.path().join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();
        fs::create_dir_all(project_dir.path().join("Controllers")).unwrap();
        fs::write(
            project_dir.path().join("Controllers/UsersController.cs"),
            r#"
[ApiController]
[Route("api/[controller]")]
public class UsersController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll() { return Ok(); }
}
"#,
        )
        .unwrap();

        // Create appsettings for multiple environments
        fs::write(
            project_dir.path().join("appsettings.Development.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://localhost:5001"}}}}"#,
        )
        .unwrap();
        fs::write(
            project_dir.path().join("appsettings.Production.json"),
            r#"{"Kestrel": {"Endpoints": {"Https": {"Url": "https://api.myapp.com"}}}}"#,
        )
        .unwrap();

        let (_scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();
        let collection = collection.expect("should have collection");

        // Should have 2 environments discovered
        assert_eq!(collection.environments.len(), 2);
        assert!(collection.environments.contains_key("dev"));
        assert!(collection.environments.contains_key("prod"));

        let dev = &collection.environments["dev"];
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");

        let prod = &collection.environments["prod"];
        assert_eq!(prod.variables["schema"], "https");
        assert_eq!(prod.variables["baseUrl"], "api.myapp.com");

        // active_env should be first discovered (dev)
        assert_eq!(collection.metadata.active_env, Some("dev".to_string()));
    }

    #[test]
    fn scan_and_create_collection_discovers_express_envs() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        fs::write(
            project_dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();
        fs::create_dir_all(project_dir.path().join("routes")).unwrap();
        fs::write(
            project_dir.path().join("routes/api.js"),
            "const router = require('express').Router();\nrouter.get('/api/health', (req, res) => res.json({ok: true}));\nmodule.exports = router;\n",
        )
        .unwrap();

        // Create .env files
        fs::write(project_dir.path().join(".env"), "PORT=3000\n").unwrap();
        fs::write(
            project_dir.path().join(".env.production"),
            "API_BASE_URL=https://api.prod.example.com\n",
        )
        .unwrap();

        let (_scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();
        let collection = collection.expect("should have collection");

        assert!(collection.environments.contains_key("dev"));
        assert!(collection.environments.contains_key("prod"));

        let dev = &collection.environments["dev"];
        assert_eq!(dev.variables["baseUrl"], "localhost:3000");

        let prod = &collection.environments["prod"];
        assert_eq!(prod.variables["baseUrl"], "api.prod.example.com");
    }

    #[test]
    fn scan_and_create_collection_falls_back_to_default_dev() {
        let project_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();

        // Express project with NO .env files
        fs::write(
            project_dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();
        fs::create_dir_all(project_dir.path().join("routes")).unwrap();
        fs::write(
            project_dir.path().join("routes/api.js"),
            "const router = require('express').Router();\nrouter.get('/status', (req, res) => res.json({}));\nmodule.exports = router;\n",
        )
        .unwrap();

        let (_scan, collection) =
            scan_and_create_collection(project_dir.path(), output_dir.path()).unwrap();
        let collection = collection.expect("should have collection");

        // Should fall back to default dev.yaml
        assert_eq!(collection.environments.len(), 1);
        assert!(collection.environments.contains_key("dev"));
        let dev = &collection.environments["dev"];
        assert_eq!(dev.variables["schema"], "http");
        assert_eq!(dev.variables["baseUrl"], "localhost:3000");
    }

    #[test]
    fn endpoint_to_request_maps_all_fields() {
        let ep = DiscoveredEndpoint {
            group: "users".to_string(),
            method: "POST".to_string(),
            route: "/api/users".to_string(),
            name: "CreateUser".to_string(),
            headers: vec![("Content-Type".to_string(), String::new())],
            query_params: vec![("page".to_string(), String::new())],
            body_type: Some("CreateUserDto".to_string()),
            body_fields: vec![
                ("Name".to_string(), "string".to_string()),
                ("Age".to_string(), "int".to_string()),
            ],
            response_type: None,
            response_fields: Vec::new(),
        };
        let req = endpoint_to_request(&ep);
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "{{schema}}://{{baseUrl}}/api/users");
        assert_eq!(req.name, "CreateUser");
        assert_eq!(req.headers.get("Content-Type").unwrap(), "");
        assert_eq!(req.params.get("page").unwrap(), "");

        // Body should be populated from body_fields
        let body = req.body.unwrap();
        assert_eq!(body.body_type, BodyType::Json);
        let content = body.content.as_object().unwrap();
        assert_eq!(
            content.get("name").unwrap(),
            &serde_json::Value::String(String::new())
        );
        assert_eq!(content.get("age").unwrap(), &serde_json::json!(0));
    }

    #[test]
    fn to_camel_case_converts_pascal() {
        assert_eq!(to_camel_case("Name"), "name");
        assert_eq!(to_camel_case("BreweryId"), "breweryId");
        assert_eq!(to_camel_case("id"), "id");
        assert_eq!(to_camel_case(""), "");
    }

    #[test]
    fn csharp_type_default_maps_types() {
        assert_eq!(csharp_type_default("string"), serde_json::json!(""));
        assert_eq!(csharp_type_default("int"), serde_json::json!(0));
        assert_eq!(csharp_type_default("double"), serde_json::json!(0));
        assert_eq!(csharp_type_default("bool"), serde_json::json!(false));
        assert_eq!(csharp_type_default("Guid"), serde_json::json!(""));
        assert_eq!(csharp_type_default("string?"), serde_json::json!(""));
        assert_eq!(csharp_type_default("int?"), serde_json::json!(0));
        assert_eq!(csharp_type_default("List<Guid>"), serde_json::json!([]));
        assert_eq!(
            csharp_type_default("DateTime?"),
            serde_json::json!("2025-01-01T00:00:00Z")
        );
    }
}
