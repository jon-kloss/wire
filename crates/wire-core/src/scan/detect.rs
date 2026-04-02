use super::types::Framework;
use std::path::Path;

/// Detect the project framework from file signatures in the directory tree.
///
/// Detection rules:
/// - `.csproj` file found anywhere → ASP.NET
/// - `package.json` with `express` dependency → Express
/// - `package.json` with `next` dependency → Next.js
/// - Neither → Unknown
pub fn detect_framework(project_dir: &Path) -> Framework {
    if has_csproj(project_dir) {
        return Framework::AspNet;
    }
    if has_express_dependency(project_dir) {
        return Framework::Express;
    }
    if has_nextjs_dependency(project_dir) {
        return Framework::NextJs;
    }
    if has_spring_boot_dependency(project_dir) {
        return Framework::SpringBoot;
    }
    if has_fastapi_dependency(project_dir) {
        return Framework::FastApi;
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

/// Check if package.json exists and contains "next" as a dependency.
fn has_nextjs_dependency(dir: &Path) -> bool {
    let pkg_path = dir.join("package.json");
    if !pkg_path.exists() {
        return false;
    }
    let content = match std::fs::read_to_string(&pkg_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    content.contains("\"next\"")
}

/// Check if pom.xml or build.gradle(.kts) contains a Spring Boot dependency.
fn has_spring_boot_dependency(dir: &Path) -> bool {
    // Check pom.xml for spring-boot-starter-web
    let pom_path = dir.join("pom.xml");
    if pom_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pom_path) {
            if content.contains("spring-boot-starter-web") {
                return true;
            }
        }
    }

    // Check build.gradle or build.gradle.kts for spring-boot
    for name in &["build.gradle", "build.gradle.kts"] {
        let gradle_path = dir.join(name);
        if gradle_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&gradle_path) {
                if content.contains("spring-boot") {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if any Python file in the project imports FastAPI.
fn has_fastapi_dependency(dir: &Path) -> bool {
    // Check requirements.txt or pyproject.toml for fastapi
    let req_path = dir.join("requirements.txt");
    if req_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&req_path) {
            if content.contains("fastapi") {
                return true;
            }
        }
    }

    let pyproject_path = dir.join("pyproject.toml");
    if pyproject_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
            if content.contains("fastapi") {
                return true;
            }
        }
    }

    // Fallback: scan .py files at top level for fastapi imports
    scan_for_python_import(dir, "fastapi", 2)
}

/// Scan Python files for a specific import, up to max_depth levels.
fn scan_for_python_import(dir: &Path, module: &str, max_depth: u32) -> bool {
    if max_depth == 0 {
        return false;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    let import_from = format!("from {module} import");
    let import_direct = format!("import {module}");
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "node_modules"
                    | ".git"
                    | "__pycache__"
                    | ".venv"
                    | "venv"
                    | ".tox"
                    | "dist"
                    | "build"
                    | ".wire"
            ) {
                continue;
            }
            if scan_for_python_import(&path, module, max_depth - 1) {
                return true;
            }
        } else if path.extension().is_some_and(|e| e == "py") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains(&import_from) || content.contains(&import_direct) {
                    return true;
                }
            }
        }
    }
    false
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
    fn detect_nextjs_from_package_json() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"next": "14.0.0", "react": "18.0.0"}}"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::NextJs);
    }

    #[test]
    fn express_takes_priority_over_nextjs() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.18.0", "next": "14.0.0"}}"#,
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
    fn detect_springboot_from_pom_xml() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pom.xml"),
            r#"<project>
                <dependencies>
                    <dependency>
                        <groupId>org.springframework.boot</groupId>
                        <artifactId>spring-boot-starter-web</artifactId>
                    </dependency>
                </dependencies>
            </project>"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::SpringBoot);
    }

    #[test]
    fn detect_springboot_from_build_gradle() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("build.gradle"),
            r#"plugins {
                id 'org.springframework.boot' version '3.2.0'
            }
            dependencies {
                implementation 'org.springframework.boot:spring-boot-starter-web'
            }"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::SpringBoot);
    }

    #[test]
    fn detect_springboot_from_build_gradle_kts() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("build.gradle.kts"),
            r#"plugins {
                id("org.springframework.boot") version "3.2.0"
            }
            dependencies {
                implementation("org.springframework.boot:spring-boot-starter-web")
            }"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::SpringBoot);
    }

    #[test]
    fn non_spring_pom_returns_unknown() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pom.xml"),
            r#"<project>
                <dependencies>
                    <dependency>
                        <groupId>junit</groupId>
                        <artifactId>junit</artifactId>
                    </dependency>
                </dependencies>
            </project>"#,
        )
        .unwrap();

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

    #[test]
    fn detect_fastapi_from_requirements_txt() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "fastapi==0.104.0\nuvicorn==0.24.0\n",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::FastApi);
    }

    #[test]
    fn detect_fastapi_from_pyproject_toml() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            r#"[project]
name = "myapi"
dependencies = ["fastapi>=0.100", "uvicorn"]
"#,
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::FastApi);
    }

    #[test]
    fn detect_fastapi_from_python_import() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("main.py"),
            "from fastapi import FastAPI\n\napp = FastAPI()\n",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::FastApi);
    }

    #[test]
    fn non_fastapi_python_returns_unknown() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "flask==3.0.0\nrequests==2.31.0\n",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }
}
