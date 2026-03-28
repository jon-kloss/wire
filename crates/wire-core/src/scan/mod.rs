mod aspnet;
mod detect;
mod express;
pub mod types;

use crate::collection::{load_collection, LoadedCollection, WireCollection, WireRequest};
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
    std::fs::create_dir_all(wire_dir.join("envs"))?;
    std::fs::create_dir_all(wire_dir.join("requests"))?;

    // Write collection metadata
    let metadata = WireCollection {
        name: project_name,
        version: 1,
        active_env: None,
    };
    let metadata_yaml = serde_yaml::to_string(&metadata)?;
    std::fs::write(wire_dir.join("wire.yaml"), metadata_yaml)?;

    // Write each endpoint as a .wire.yaml request file
    for endpoint in &scan.endpoints {
        let request = endpoint_to_request(endpoint);
        let filename = slugify(&endpoint.name) + ".wire.yaml";
        let file_path = wire_dir.join("requests").join(&filename);

        // Avoid overwriting duplicates — append a number
        let final_path = unique_path(&file_path);
        let yaml = serde_yaml::to_string(&request)?;
        std::fs::write(&final_path, yaml)?;
    }

    let collection = load_collection(&wire_dir)?;
    Ok((scan, Some(collection)))
}

/// Convert a DiscoveredEndpoint to a WireRequest.
fn endpoint_to_request(endpoint: &DiscoveredEndpoint) -> WireRequest {
    let mut headers = std::collections::HashMap::new();
    for (name, _) in &endpoint.headers {
        headers.insert(name.clone(), String::new());
    }

    let mut params = std::collections::HashMap::new();
    for (name, _) in &endpoint.query_params {
        params.insert(name.clone(), String::new());
    }

    WireRequest {
        name: endpoint.name.clone(),
        method: endpoint.method.clone(),
        url: endpoint.route.clone(),
        headers,
        params,
        body: None,
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

        // Verify route params converted
        let routes: Vec<&str> = collection
            .requests
            .iter()
            .map(|(_, r)| r.url.as_str())
            .collect();
        assert!(routes.contains(&"/api/items"));
        assert!(routes.contains(&"/api/items/{{id}}"));
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
    fn endpoint_to_request_maps_all_fields() {
        let ep = DiscoveredEndpoint {
            method: "POST".to_string(),
            route: "/api/users".to_string(),
            name: "CreateUser".to_string(),
            headers: vec![("Content-Type".to_string(), String::new())],
            query_params: vec![("page".to_string(), String::new())],
            body_type: Some("CreateUserDto".to_string()),
        };
        let req = endpoint_to_request(&ep);
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "/api/users");
        assert_eq!(req.name, "CreateUser");
        assert_eq!(req.headers.get("Content-Type").unwrap(), "");
        assert_eq!(req.params.get("page").unwrap(), "");
    }
}
