mod aspnet;
mod detect;
mod express;
pub mod types;

use crate::error::WireError;
use std::path::Path;
use types::{Framework, ScanResult};

pub use detect::detect_framework;

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
}
