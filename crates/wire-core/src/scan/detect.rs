use super::types::Framework;
use std::path::Path;

/// Detect the project framework from file signatures in the directory tree.
///
/// Detection rules:
/// - `.csproj` file found anywhere → ASP.NET
/// - `package.json` with `express` dependency → Express
/// - Neither → Unknown
pub fn detect_framework(project_dir: &Path) -> Framework {
    if has_csproj(project_dir) {
        return Framework::AspNet;
    }
    if has_express_dependency(project_dir) {
        return Framework::Express;
    }
    Framework::Unknown
}

/// Check for any .csproj file in the project tree (max 3 levels deep to avoid scanning node_modules etc).
fn has_csproj(dir: &Path) -> bool {
    scan_for_extension(dir, "csproj", 3)
}

/// Check if package.json exists and contains "express" as a dependency.
fn has_express_dependency(dir: &Path) -> bool {
    let pkg_path = dir.join("package.json");
    if !pkg_path.exists() {
        return false;
    }
    let content = match std::fs::read_to_string(&pkg_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Check both "dependencies" and "devDependencies" for "express"
    // Simple string check — sufficient for detection without pulling in a JSON parser
    content.contains("\"express\"")
}

/// Recursively scan for files with the given extension, up to max_depth levels.
fn scan_for_extension(dir: &Path, ext: &str, max_depth: u32) -> bool {
    if max_depth == 0 {
        return false;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // Skip common large/irrelevant directories
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "node_modules" | ".git" | "bin" | "obj" | "target" | ".wire" | "dist" | "build"
            ) {
                continue;
            }
            if scan_for_extension(&path, ext, max_depth - 1) {
                return true;
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detect_aspnet_from_csproj() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::AspNet);
    }

    #[test]
    fn detect_aspnet_from_nested_csproj() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("src/MyApi");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::AspNet);
    }

    #[test]
    fn detect_express_from_package_json() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Express);
    }

    #[test]
    fn detect_express_from_dev_dependencies() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies": {"express": "^4.18.0", "jest": "^29.0.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Express);
    }

    #[test]
    fn detect_unknown_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }

    #[test]
    fn detect_unknown_for_package_json_without_express() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"fastify": "^4.0.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }

    #[test]
    fn aspnet_takes_priority_over_express() {
        // If both are present, ASP.NET wins (checked first)
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("MyApi.csproj"), "<Project></Project>").unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::AspNet);
    }

    #[test]
    fn skips_node_modules_directory() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules/some-pkg");
        fs::create_dir_all(&nm).unwrap();
        // .csproj inside node_modules should not be detected
        fs::write(nm.join("fake.csproj"), "<Project></Project>").unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }

    #[test]
    fn respects_max_depth() {
        let dir = TempDir::new().unwrap();
        // 4 levels deep — deeper than max_depth of 3
        let deep = dir.path().join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Deep.csproj"), "<Project></Project>").unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }
}
