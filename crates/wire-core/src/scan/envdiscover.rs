use std::collections::HashMap;
use std::path::Path;

use super::types::Framework;

/// A discovered environment configuration from project config files.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredEnvironment {
    /// Display name (e.g., "Development", "Production")
    pub name: String,
    /// Filename slug for the .yaml file (e.g., "dev", "prod")
    pub filename: String,
    /// Discovered variables (e.g., schema, baseUrl, port)
    pub variables: HashMap<String, String>,
}

/// Discover environment configurations from project config files.
///
/// For ASP.NET: scans appsettings.*.json and Properties/launchSettings.json
/// For Express: scans .env* files
/// Returns empty vec if no config files found.
pub fn discover_environments(
    project_dir: &Path,
    framework: &Framework,
) -> Vec<DiscoveredEnvironment> {
    match framework {
        Framework::AspNet => discover_aspnet_envs(project_dir),
        Framework::Express | Framework::NextJs => discover_express_envs(project_dir),
        Framework::Unknown => {
            // Combine results from all scanners (matches scan_project behavior)
            let mut envs = discover_aspnet_envs(project_dir);
            let express_envs = discover_express_envs(project_dir);
            for env in express_envs {
                if !envs.iter().any(|e| e.filename == env.filename) {
                    envs.push(env);
                }
            }
            envs
        }
    }
}

/// Map ASP.NET environment names to short slugs.
fn aspnet_env_slug(env_name: &str) -> (&str, &str) {
    match env_name.to_lowercase().as_str() {
        "development" => ("dev", "Development"),
        "staging" => ("stage", "Staging"),
        "production" => ("prod", "Production"),
        "integration" => ("int", "Integration"),
        _ => {
            // Return as-is for unknown environments; caller handles lifetime
            ("", "")
        }
    }
}

// Keys to skip (secrets/sensitive data)
fn is_sensitive_key(key: &str) -> bool {
    // Split on common delimiters and check each word segment
    let segments: Vec<String> = key
        .split(['_', '-', '.'])
        .map(|s| s.to_lowercase())
        .collect();

    // Also check camelCase by splitting on uppercase boundaries
    let camel_lower = key.to_lowercase();

    segments.iter().any(|s| {
        s == "password"
            || s == "secret"
            || s == "key"
            || s == "token"
            || s == "credential"
            || s == "credentials"
    }) || camel_lower.contains("password")
        || camel_lower.contains("secret")
        || camel_lower.contains("connectionstring")
}

/// Find the directory containing the .csproj file (searches up to 3 levels deep).
/// Falls back to project_dir if no .csproj found.
fn find_aspnet_project_dir(project_dir: &Path) -> std::path::PathBuf {
    fn search(dir: &Path, depth: u32) -> Option<std::path::PathBuf> {
        if depth == 0 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if path.is_file() && name.ends_with(".csproj") {
                return Some(dir.to_path_buf());
            }
            if path.is_dir()
                && !matches!(
                    name.as_str(),
                    "node_modules" | ".git" | "bin" | "obj" | "target" | ".wire" | "dist" | "build"
                )
            {
                subdirs.push(path);
            }
        }
        for sub in subdirs {
            if let Some(found) = search(&sub, depth - 1) {
                return Some(found);
            }
        }
        None
    }
    search(project_dir, 4).unwrap_or_else(|| project_dir.to_path_buf())
}

/// Discover ASP.NET environments from appsettings.*.json and launchSettings.json.
fn discover_aspnet_envs(project_dir: &Path) -> Vec<DiscoveredEnvironment> {
    let mut envs: Vec<DiscoveredEnvironment> = Vec::new();
    let mut base_settings: Option<serde_json::Value> = None;

    // Find the actual ASP.NET project directory (where .csproj lives)
    let aspnet_dir = find_aspnet_project_dir(project_dir);

    // 1. Parse base appsettings.json for defaults
    let base_path = aspnet_dir.join("appsettings.json");
    if base_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&base_path) {
            base_settings = serde_json::from_str(&content).ok();
        }
    }

    // 2. Find appsettings.{Environment}.json files
    if let Ok(entries) = std::fs::read_dir(&aspnet_dir) {
        let mut env_files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("appsettings.")
                    && name.ends_with(".json")
                    && name != "appsettings.json"
            })
            .collect();
        env_files.sort_by_key(|e| e.file_name());

        for entry in env_files {
            let filename = entry.file_name().to_string_lossy().to_string();
            // Extract environment name: appsettings.Development.json -> Development
            let env_name = filename
                .strip_prefix("appsettings.")
                .and_then(|s| s.strip_suffix(".json"))
                .unwrap_or("")
                .to_string();

            if env_name.is_empty() {
                continue;
            }

            let (slug, display) = aspnet_env_slug(&env_name);
            // For unknown env names, use lowercase as slug
            let slug = if slug.is_empty() {
                env_name.to_lowercase()
            } else {
                slug.to_string()
            };
            let display = if display.is_empty() {
                env_name.clone()
            } else {
                display.to_string()
            };

            let mut variables = HashMap::new();

            // Merge base settings then override with env-specific
            if let Some(ref base) = base_settings {
                extract_aspnet_urls(base, &mut variables);
            }

            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    extract_aspnet_urls(&json, &mut variables);
                }
            }

            if !variables.is_empty() {
                envs.push(DiscoveredEnvironment {
                    name: display,
                    filename: slug,
                    variables,
                });
            }
        }
    }

    // 3. Parse Properties/launchSettings.json for dev environment
    let launch_path = aspnet_dir.join("Properties").join("launchSettings.json");
    if launch_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&launch_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(variables) = extract_launch_settings_urls(&json) {
                    // Merge into existing dev env or create new
                    if let Some(dev_env) = envs.iter_mut().find(|e| e.filename == "dev") {
                        for (k, v) in &variables {
                            dev_env.variables.entry(k.clone()).or_insert(v.clone());
                        }
                    } else {
                        envs.insert(
                            0,
                            DiscoveredEnvironment {
                                name: "Development".to_string(),
                                filename: "dev".to_string(),
                                variables,
                            },
                        );
                    }
                }
            }
        }
    }

    envs
}

/// Extract URLs from ASP.NET appsettings JSON.
fn extract_aspnet_urls(json: &serde_json::Value, variables: &mut HashMap<String, String>) {
    // Check Kestrel endpoints
    if let Some(endpoints) = json
        .get("Kestrel")
        .and_then(|k| k.get("Endpoints"))
        .and_then(|e| e.as_object())
    {
        for (_name, endpoint) in endpoints {
            if let Some(url) = endpoint.get("Url").and_then(|u| u.as_str()) {
                if let Some((schema, host_port)) = parse_url_parts(url) {
                    variables.insert("schema".to_string(), schema);
                    variables.insert("baseUrl".to_string(), host_port);
                    break; // Use first endpoint
                }
            }
        }
    }

    // Check Urls array (alternative Kestrel config)
    if !variables.contains_key("baseUrl") {
        if let Some(urls) = json.get("Urls").and_then(|u| u.as_str()) {
            if let Some(first_url) = urls.split(';').next() {
                if let Some((schema, host_port)) = parse_url_parts(first_url.trim()) {
                    variables.insert("schema".to_string(), schema);
                    variables.insert("baseUrl".to_string(), host_port);
                }
            }
        }
    }

    // Check common top-level URL keys (e.g., "BaseUrl", "ApplicationUrl")
    if !variables.contains_key("baseUrl") {
        let url_keys = [
            "BaseUrl",
            "baseUrl",
            "ApplicationUrl",
            "ApiUrl",
            "ServerUrl",
        ];
        for key in url_keys {
            if let Some(url) = json.get(key).and_then(|u| u.as_str()) {
                if let Some((schema, host_port)) = parse_url_parts(url) {
                    variables.insert("schema".to_string(), schema);
                    variables.insert("baseUrl".to_string(), host_port);
                    break;
                }
            }
        }
    }

    // Extract non-sensitive app settings
    if let Some(app_settings) = json.get("AppSettings").and_then(|a| a.as_object()) {
        for (key, value) in app_settings {
            if !is_sensitive_key(key) {
                if let Some(val) = value.as_str() {
                    variables.insert(key.clone(), val.to_string());
                }
            }
        }
    }
}

/// Extract URLs from launchSettings.json profiles.
fn extract_launch_settings_urls(json: &serde_json::Value) -> Option<HashMap<String, String>> {
    let profiles = json.get("profiles")?.as_object()?;

    for (_name, profile) in profiles {
        if let Some(app_url) = profile.get("applicationUrl").and_then(|u| u.as_str()) {
            // applicationUrl can have multiple URLs separated by ;
            // Prefer http for local dev
            let urls: Vec<&str> = app_url.split(';').map(|s| s.trim()).collect();
            let url = urls
                .iter()
                .find(|u| u.starts_with("http://"))
                .or(urls.first())
                .copied()?;

            if let Some((schema, host_port)) = parse_url_parts(url) {
                let mut variables = HashMap::new();
                variables.insert("schema".to_string(), schema);
                variables.insert("baseUrl".to_string(), host_port);
                return Some(variables);
            }
        }
    }
    None
}

/// Parse a URL into (schema, host:port) parts.
fn parse_url_parts(url: &str) -> Option<(String, String)> {
    let (schema, rest) = url.split_once("://")?;
    // Strip trailing slash
    let host_port = rest.trim_end_matches('/').to_string();
    Some((schema.to_string(), host_port))
}

/// Discover Express/Node.js environments from .env* files.
fn discover_express_envs(project_dir: &Path) -> Vec<DiscoveredEnvironment> {
    let mut envs: Vec<DiscoveredEnvironment> = Vec::new();

    // Env file -> (slug, display_name)
    let env_files = [
        (".env", "dev", "Development"),
        (".env.development", "dev", "Development"),
        (".env.staging", "stage", "Staging"),
        (".env.production", "prod", "Production"),
        (".env.test", "test", "Test"),
    ];

    for (file, slug, display) in env_files {
        let file_path = project_dir.join(file);
        if !file_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let variables = parse_dotenv(&content);
        if variables.is_empty() {
            continue;
        }

        // Merge into existing env with same slug or create new
        if let Some(existing) = envs.iter_mut().find(|e| e.filename == slug) {
            for (k, v) in &variables {
                existing.variables.insert(k.clone(), v.clone());
            }
        } else {
            envs.push(DiscoveredEnvironment {
                name: display.to_string(),
                filename: slug.to_string(),
                variables,
            });
        }
    }

    envs
}

/// Parse a .env file and extract relevant Wire variables.
/// Only extracts URL/port-related keys, skips secrets.
fn parse_dotenv(content: &str) -> HashMap<String, String> {
    let mut raw_vars: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();

            // Strip surrounding quotes (skip inline comment stripping for quoted values)
            let was_quoted = (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''));
            if was_quoted {
                value = value[1..value.len() - 1].to_string();
            } else {
                // Strip inline comments only for unquoted values
                if let Some(comment_pos) = value.find(" #") {
                    value = value[..comment_pos].trim().to_string();
                }
            }

            if !key.is_empty() {
                raw_vars.insert(key, value);
            }
        }
    }

    // Map relevant keys to Wire variables
    let mut variables = HashMap::new();

    // Extract port
    if let Some(port) = raw_vars.get("PORT") {
        variables.insert("port".to_string(), port.clone());
    }

    // Extract base URL from various common keys
    let url_keys = [
        "BASE_URL",
        "API_BASE_URL",
        "API_URL",
        "APP_URL",
        "SERVER_URL",
    ];
    for key in url_keys {
        if let Some(url) = raw_vars.get(key) {
            if let Some((schema, host_port)) = parse_url_parts(url) {
                variables.insert("schema".to_string(), schema);
                variables.insert("baseUrl".to_string(), host_port);
                break;
            }
        }
    }

    // If no URL found but we have port + host, construct baseUrl
    if !variables.contains_key("baseUrl") {
        let host = raw_vars
            .get("HOST")
            .cloned()
            .unwrap_or_else(|| "localhost".to_string());
        if let Some(port) = raw_vars.get("PORT") {
            variables
                .entry("schema".to_string())
                .or_insert_with(|| "http".to_string());
            variables.insert("baseUrl".to_string(), format!("{host}:{port}"));
        }
    }

    variables
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- ASP.NET tests ---

    #[test]
    fn aspnet_discovers_envs_from_appsettings() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("appsettings.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://localhost:5000"}}}}"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("appsettings.Development.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://localhost:5001"}}}}"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("appsettings.Production.json"),
            r#"{"Kestrel": {"Endpoints": {"Https": {"Url": "https://api.example.com"}}}}"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 2);

        let dev = envs.iter().find(|e| e.filename == "dev").unwrap();
        assert_eq!(dev.name, "Development");
        assert_eq!(dev.variables["schema"], "http");
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");

        let prod = envs.iter().find(|e| e.filename == "prod").unwrap();
        assert_eq!(prod.name, "Production");
        assert_eq!(prod.variables["schema"], "https");
        assert_eq!(prod.variables["baseUrl"], "api.example.com");
    }

    #[test]
    fn aspnet_discovers_launch_settings() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("Properties")).unwrap();

        fs::write(
            dir.path().join("Properties/launchSettings.json"),
            r#"{
                "profiles": {
                    "https": {
                        "applicationUrl": "https://localhost:7001;http://localhost:5001"
                    }
                }
            }"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 1);

        let dev = &envs[0];
        assert_eq!(dev.filename, "dev");
        assert_eq!(dev.variables["schema"], "http");
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");
    }

    #[test]
    fn aspnet_merges_launch_settings_into_existing_dev() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("Properties")).unwrap();

        fs::write(
            dir.path().join("appsettings.Development.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://localhost:5001"}}}}"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("Properties/launchSettings.json"),
            r#"{"profiles": {"http": {"applicationUrl": "http://localhost:5099"}}}"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 1);

        // appsettings.Development takes priority (already set)
        let dev = &envs[0];
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");
    }

    #[test]
    fn aspnet_extracts_urls_config() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("appsettings.Staging.json"),
            r#"{"Urls": "http://staging.example.com:8080"}"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].filename, "stage");
        assert_eq!(envs[0].variables["baseUrl"], "staging.example.com:8080");
    }

    #[test]
    fn aspnet_skips_sensitive_app_settings() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("appsettings.Development.json"),
            r#"{
                "Kestrel": {"Endpoints": {"Http": {"Url": "http://localhost:5000"}}},
                "AppSettings": {
                    "ApiVersion": "v2",
                    "SecretKey": "super-secret",
                    "DatabasePassword": "pass123"
                }
            }"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        let dev = &envs[0];
        assert!(dev.variables.contains_key("ApiVersion"));
        assert!(!dev.variables.contains_key("SecretKey"));
        assert!(!dev.variables.contains_key("DatabasePassword"));
    }

    #[test]
    fn sensitive_key_does_not_false_positive() {
        // Should be sensitive
        assert!(is_sensitive_key("SecretKey"));
        assert!(is_sensitive_key("api_token"));
        assert!(is_sensitive_key("DatabasePassword"));
        assert!(is_sensitive_key("AWS_SECRET_KEY"));
        // Should NOT be sensitive
        assert!(!is_sensitive_key("ApiVersion"));
        assert!(!is_sensitive_key("MonkeyCount"));
        assert!(!is_sensitive_key("Hockey"));
        assert!(!is_sensitive_key("PublicEndpoint"));
        assert!(!is_sensitive_key("FeatureXEnabled"));
    }

    #[test]
    fn aspnet_discovers_envs_in_nested_project() {
        let dir = TempDir::new().unwrap();
        let api_dir = dir.path().join("backend/src/MyApi");
        fs::create_dir_all(&api_dir).unwrap();
        fs::create_dir_all(api_dir.join("Properties")).unwrap();

        fs::write(
            api_dir.join("MyApi.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk.Web\"></Project>",
        )
        .unwrap();
        fs::write(
            api_dir.join("appsettings.Development.json"),
            r#"{"BaseUrl": "https://localhost:5001/"}"#,
        )
        .unwrap();
        fs::write(
            api_dir.join("appsettings.Production.json"),
            r#"{"BaseUrl": "https://api.example.com/"}"#,
        )
        .unwrap();
        fs::write(
            api_dir.join("Properties/launchSettings.json"),
            r#"{"profiles": {"http": {"applicationUrl": "http://localhost:5004"}}}"#,
        )
        .unwrap();

        // Pass the root dir — should find appsettings in nested backend/src/MyApi/
        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 2);

        let dev = envs.iter().find(|e| e.filename == "dev").unwrap();
        assert_eq!(dev.variables["schema"], "https");
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");

        let prod = envs.iter().find(|e| e.filename == "prod").unwrap();
        assert_eq!(prod.variables["baseUrl"], "api.example.com");
    }

    #[test]
    fn aspnet_discovers_top_level_base_url() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("appsettings.Development.json"),
            r#"{"BaseUrl": "https://localhost:5001/", "Logging": {"LogLevel": {"Default": "Information"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("appsettings.Production.json"),
            r#"{"BaseUrl": "https://api.example.com/", "Logging": {"LogLevel": {"Default": "Warning"}}}"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 2);

        let dev = envs.iter().find(|e| e.filename == "dev").unwrap();
        assert_eq!(dev.variables["schema"], "https");
        assert_eq!(dev.variables["baseUrl"], "localhost:5001");

        let prod = envs.iter().find(|e| e.filename == "prod").unwrap();
        assert_eq!(prod.variables["schema"], "https");
        assert_eq!(prod.variables["baseUrl"], "api.example.com");
    }

    #[test]
    fn aspnet_empty_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let envs = discover_aspnet_envs(dir.path());
        assert!(envs.is_empty());
    }

    #[test]
    fn aspnet_unknown_env_name_uses_lowercase() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("appsettings.QA.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://qa.example.com:5000"}}}}"#,
        )
        .unwrap();

        let envs = discover_aspnet_envs(dir.path());
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].filename, "qa");
        assert_eq!(envs[0].name, "QA");
    }

    // --- Express tests ---

    #[test]
    fn express_discovers_env_files() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join(".env"), "PORT=3000\nHOST=localhost\n").unwrap();

        fs::write(
            dir.path().join(".env.production"),
            "API_BASE_URL=https://api.example.com\n",
        )
        .unwrap();

        let envs = discover_express_envs(dir.path());
        assert_eq!(envs.len(), 2);

        let dev = envs.iter().find(|e| e.filename == "dev").unwrap();
        assert_eq!(dev.variables["schema"], "http");
        assert_eq!(dev.variables["baseUrl"], "localhost:3000");

        let prod = envs.iter().find(|e| e.filename == "prod").unwrap();
        assert_eq!(prod.variables["schema"], "https");
        assert_eq!(prod.variables["baseUrl"], "api.example.com");
    }

    #[test]
    fn express_merges_env_and_env_development() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join(".env"), "PORT=3000\n").unwrap();
        fs::write(
            dir.path().join(".env.development"),
            "BASE_URL=http://localhost:4000\n",
        )
        .unwrap();

        let envs = discover_express_envs(dir.path());
        // Both map to "dev" slug, should merge
        let dev = envs.iter().find(|e| e.filename == "dev").unwrap();
        assert_eq!(dev.variables["baseUrl"], "localhost:4000");
        assert_eq!(dev.variables["port"], "3000");
    }

    #[test]
    fn express_empty_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let envs = discover_express_envs(dir.path());
        assert!(envs.is_empty());
    }

    #[test]
    fn express_constructs_base_url_from_port_host() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "PORT=8080\n").unwrap();

        let envs = discover_express_envs(dir.path());
        let dev = &envs[0];
        assert_eq!(dev.variables["baseUrl"], "localhost:8080");
        assert_eq!(dev.variables["schema"], "http");
    }

    // --- dotenv parser tests ---

    #[test]
    fn parse_dotenv_handles_comments_and_blanks() {
        let content = "# Comment\n\nPORT=3000\n# Another comment\nBASE_URL=http://localhost:3000\n";
        let vars = parse_dotenv(content);
        assert_eq!(vars["port"], "3000");
        assert_eq!(vars["baseUrl"], "localhost:3000");
    }

    #[test]
    fn parse_dotenv_strips_quotes() {
        let content = "BASE_URL=\"http://example.com:8080\"\nPORT='4000'\n";
        let vars = parse_dotenv(content);
        assert_eq!(vars["baseUrl"], "example.com:8080");
        assert_eq!(vars["port"], "4000");
    }

    #[test]
    fn parse_dotenv_handles_inline_comments() {
        let content = "PORT=3000 # server port\n";
        let vars = parse_dotenv(content);
        assert_eq!(vars["port"], "3000");
    }

    #[test]
    fn parse_dotenv_ignores_non_url_keys() {
        let content = "PORT=3000\nDB_HOST=localhost\nSOME_RANDOM=value\n";
        let vars = parse_dotenv(content);
        assert!(vars.contains_key("port"));
        assert!(vars.contains_key("baseUrl")); // constructed from PORT
        assert!(!vars.contains_key("DB_HOST"));
        assert!(!vars.contains_key("SOME_RANDOM"));
    }

    // --- discover_environments integration tests ---

    #[test]
    fn discover_environments_aspnet_framework() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("appsettings.Production.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "https://prod.example.com"}}}}"#,
        )
        .unwrap();

        let envs = discover_environments(dir.path(), &Framework::AspNet);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].filename, "prod");
    }

    #[test]
    fn discover_environments_express_framework() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "PORT=3000\n").unwrap();

        let envs = discover_environments(dir.path(), &Framework::Express);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].filename, "dev");
    }

    #[test]
    fn discover_environments_unknown_combines_both() {
        let dir = TempDir::new().unwrap();

        // ASP.NET config
        fs::write(
            dir.path().join("appsettings.Staging.json"),
            r#"{"Kestrel": {"Endpoints": {"Http": {"Url": "http://staging.example.com:8080"}}}}"#,
        )
        .unwrap();

        // Express config
        fs::write(
            dir.path().join(".env.production"),
            "API_URL=https://api.example.com\n",
        )
        .unwrap();

        let envs = discover_environments(dir.path(), &Framework::Unknown);
        assert_eq!(envs.len(), 2);
        assert!(envs.iter().any(|e| e.filename == "stage"));
        assert!(envs.iter().any(|e| e.filename == "prod"));
    }
}
