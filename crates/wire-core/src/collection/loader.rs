use crate::collection::{Environment, WireCollection, WireRequest};
use crate::error::WireError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A fully loaded .wire/ collection directory.
#[derive(Debug)]
pub struct LoadedCollection {
    pub metadata: WireCollection,
    pub requests: Vec<(PathBuf, WireRequest)>,
    pub environments: HashMap<String, Environment>,
}

/// Create a new .wire/ collection directory with the given name.
/// Creates wire.yaml metadata, envs/, and requests/ directories.
/// Returns the loaded collection.
pub fn create_collection(parent_dir: &Path, name: &str) -> Result<LoadedCollection, WireError> {
    let wire_dir = parent_dir.join(".wire");

    // Guard: do not overwrite an existing collection
    if wire_dir.join("wire.yaml").exists() {
        return Err(WireError::Other(format!(
            "Collection already exists at {}",
            wire_dir.display()
        )));
    }

    std::fs::create_dir_all(wire_dir.join("envs"))?;
    std::fs::create_dir_all(wire_dir.join("requests"))?;

    // Use serde to properly escape the collection name in YAML
    let metadata_obj = WireCollection {
        name: name.to_string(),
        version: 1,
        active_env: None,
    };
    let metadata = serde_yaml::to_string(&metadata_obj)?;
    std::fs::write(wire_dir.join("wire.yaml"), metadata)?;

    load_collection(&wire_dir)
}

/// Rename a collection by updating its wire.yaml metadata.
pub fn rename_collection(wire_dir: &Path, new_name: &str) -> Result<LoadedCollection, WireError> {
    let metadata_path = wire_dir.join("wire.yaml");
    if !metadata_path.exists() {
        return Err(WireError::Other(format!(
            "No wire.yaml found at {}",
            wire_dir.display()
        )));
    }

    let content = std::fs::read_to_string(&metadata_path)?;
    let mut metadata: WireCollection = serde_yaml::from_str(&content)?;
    metadata.name = new_name.to_string();
    let yaml = serde_yaml::to_string(&metadata)?;
    std::fs::write(&metadata_path, yaml)?;

    load_collection(wire_dir)
}

/// Load a single .wire.yaml request file.
pub fn load_request(path: &Path) -> Result<WireRequest, WireError> {
    let content = std::fs::read_to_string(path)?;
    let request: WireRequest = serde_yaml::from_str(&content)?;
    Ok(request)
}

/// Load a full .wire/ collection directory.
///
/// Expected structure:
/// ```text
/// .wire/
/// ├── wire.yaml          # collection metadata
/// ├── envs/
/// │   ├── dev.yaml
/// │   └── prod.yaml
/// └── requests/
///     ├── auth/
///     │   └── login.wire.yaml
///     └── users/
///         └── list.wire.yaml
/// ```
pub fn load_collection(wire_dir: &Path) -> Result<LoadedCollection, WireError> {
    // Load metadata
    let metadata_path = wire_dir.join("wire.yaml");
    let metadata: WireCollection = if metadata_path.exists() {
        let content = std::fs::read_to_string(&metadata_path)?;
        serde_yaml::from_str(&content)?
    } else {
        WireCollection {
            name: wire_dir
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unnamed Collection".to_string()),
            version: 1,
            active_env: None,
        }
    };

    // Load environments
    let mut environments = HashMap::new();
    let envs_dir = wire_dir.join("envs");
    if envs_dir.is_dir() {
        for entry in std::fs::read_dir(&envs_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                let content = std::fs::read_to_string(&path)?;
                let env: Environment = serde_yaml::from_str(&content)?;
                let key = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                environments.insert(key, env);
            }
        }
    }

    // Load requests recursively
    let mut requests = Vec::new();
    let requests_dir = wire_dir.join("requests");
    if requests_dir.is_dir() {
        load_requests_recursive(&requests_dir, &mut requests)?;
    }

    Ok(LoadedCollection {
        metadata,
        requests,
        environments,
    })
}

fn load_requests_recursive(
    dir: &Path,
    requests: &mut Vec<(PathBuf, WireRequest)>,
) -> Result<(), WireError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            load_requests_recursive(&path, requests)?;
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".wire.yaml"))
        {
            let request = load_request(&path)?;
            requests.push((path, request));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_sample_collection(dir: &Path) {
        let wire_dir = dir.join(".wire");
        fs::create_dir_all(wire_dir.join("envs")).unwrap();
        fs::create_dir_all(wire_dir.join("requests/auth")).unwrap();
        fs::create_dir_all(wire_dir.join("requests/users")).unwrap();

        fs::write(
            wire_dir.join("wire.yaml"),
            "name: Test Collection\nversion: 1\nactive_env: dev\n",
        )
        .unwrap();

        fs::write(
            wire_dir.join("envs/dev.yaml"),
            "name: Development\nvariables:\n  base_url: http://localhost:3000\n  token: dev-tok\n",
        )
        .unwrap();

        fs::write(
            wire_dir.join("envs/prod.yaml"),
            "name: Production\nvariables:\n  base_url: https://api.example.com\n  token: prod-tok\n",
        )
        .unwrap();

        fs::write(
            wire_dir.join("requests/auth/login.wire.yaml"),
            "name: Login\nmethod: POST\nurl: \"{{base_url}}/auth/login\"\n",
        )
        .unwrap();

        fs::write(
            wire_dir.join("requests/users/list.wire.yaml"),
            "name: List Users\nmethod: GET\nurl: \"{{base_url}}/users\"\nheaders:\n  Authorization: \"Bearer {{token}}\"\n",
        )
        .unwrap();
    }

    #[test]
    fn load_single_request_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.wire.yaml");
        fs::write(&path, "name: Test\nmethod: GET\nurl: https://example.com\n").unwrap();

        let req = load_request(&path).unwrap();
        assert_eq!(req.name, "Test");
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://example.com");
    }

    #[test]
    fn load_full_collection() {
        let dir = TempDir::new().unwrap();
        create_sample_collection(dir.path());

        let collection = load_collection(&dir.path().join(".wire")).unwrap();

        assert_eq!(collection.metadata.name, "Test Collection");
        assert_eq!(collection.metadata.version, 1);
        assert_eq!(collection.metadata.active_env, Some("dev".to_string()));

        assert_eq!(collection.environments.len(), 2);
        assert!(collection.environments.contains_key("dev"));
        assert!(collection.environments.contains_key("prod"));
        assert_eq!(
            collection.environments["dev"].variables["base_url"],
            "http://localhost:3000"
        );

        assert_eq!(collection.requests.len(), 2);
        let names: Vec<&str> = collection
            .requests
            .iter()
            .map(|(_, r)| r.name.as_str())
            .collect();
        assert!(names.contains(&"Login"));
        assert!(names.contains(&"List Users"));
    }

    #[test]
    fn load_collection_without_metadata() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(wire_dir.join("requests")).unwrap();
        fs::write(
            wire_dir.join("requests/test.wire.yaml"),
            "name: Test\nmethod: GET\nurl: https://example.com\n",
        )
        .unwrap();

        let collection = load_collection(&wire_dir).unwrap();
        assert_eq!(collection.metadata.version, 1);
        assert_eq!(collection.requests.len(), 1);
    }

    #[test]
    fn load_collection_empty_dir() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(&wire_dir).unwrap();

        let collection = load_collection(&wire_dir).unwrap();
        assert!(collection.requests.is_empty());
        assert!(collection.environments.is_empty());
    }

    #[test]
    fn load_collection_deeply_nested_requests() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(wire_dir.join("requests/api/v2/admin")).unwrap();
        fs::write(
            wire_dir.join("requests/api/v2/admin/create.wire.yaml"),
            "name: Deep Request\nmethod: POST\nurl: https://example.com/deep\n",
        )
        .unwrap();

        let collection = load_collection(&wire_dir).unwrap();
        assert_eq!(collection.requests.len(), 1);
        assert_eq!(collection.requests[0].1.name, "Deep Request");
    }

    #[test]
    fn load_collection_ignores_non_wire_yaml_files() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(wire_dir.join("requests")).unwrap();

        // This should be loaded
        fs::write(
            wire_dir.join("requests/valid.wire.yaml"),
            "name: Valid\nmethod: GET\nurl: https://example.com\n",
        )
        .unwrap();

        // These should be ignored
        fs::write(wire_dir.join("requests/notes.txt"), "some notes").unwrap();
        fs::write(
            wire_dir.join("requests/other.yaml"),
            "name: Not Wire\nmethod: GET\nurl: https://example.com\n",
        )
        .unwrap();
        fs::write(wire_dir.join("requests/readme.md"), "# Readme").unwrap();

        let collection = load_collection(&wire_dir).unwrap();
        assert_eq!(collection.requests.len(), 1);
        assert_eq!(collection.requests[0].1.name, "Valid");
    }

    #[test]
    fn load_request_malformed_yaml_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.wire.yaml");
        fs::write(&path, "this: is: not: valid: yaml: {{").unwrap();

        let result = load_request(&path);
        assert!(result.is_err());
    }

    #[test]
    fn load_request_nonexistent_file_fails() {
        let result = load_request(Path::new("/nonexistent/path/req.wire.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn create_collection_creates_directory_structure() {
        let dir = TempDir::new().unwrap();
        let collection = create_collection(dir.path(), "My API").unwrap();

        assert_eq!(collection.metadata.name, "My API");
        assert_eq!(collection.metadata.version, 1);
        assert!(collection.requests.is_empty());
        assert!(collection.environments.is_empty());

        // Verify directory structure
        let wire_dir = dir.path().join(".wire");
        assert!(wire_dir.join("wire.yaml").exists());
        assert!(wire_dir.join("envs").is_dir());
        assert!(wire_dir.join("requests").is_dir());
    }

    #[test]
    fn create_collection_then_add_request() {
        let dir = TempDir::new().unwrap();
        create_collection(dir.path(), "Test API").unwrap();

        // Save a request into the collection
        let wire_dir = dir.path().join(".wire");
        fs::write(
            wire_dir.join("requests/health.wire.yaml"),
            "name: Health\nmethod: GET\nurl: https://example.com/health\n",
        )
        .unwrap();

        // Reload and verify
        let reloaded = load_collection(&wire_dir).unwrap();
        assert_eq!(reloaded.requests.len(), 1);
        assert_eq!(reloaded.requests[0].1.name, "Health");
    }

    #[test]
    fn create_collection_fails_if_already_exists() {
        let dir = TempDir::new().unwrap();
        create_collection(dir.path(), "First").unwrap();

        // Second create on same dir should fail
        let result = create_collection(dir.path(), "Second");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("already exists"));
    }

    #[test]
    fn create_collection_with_special_yaml_chars_in_name() {
        let dir = TempDir::new().unwrap();
        let collection = create_collection(dir.path(), "My API: v2 {test}").unwrap();
        assert_eq!(collection.metadata.name, "My API: v2 {test}");
    }

    #[test]
    fn rename_collection_updates_metadata() {
        let dir = TempDir::new().unwrap();
        create_collection(dir.path(), "Original Name").unwrap();

        let wire_dir = dir.path().join(".wire");
        let renamed = rename_collection(&wire_dir, "New Name").unwrap();
        assert_eq!(renamed.metadata.name, "New Name");

        // Verify persisted on disk
        let reloaded = load_collection(&wire_dir).unwrap();
        assert_eq!(reloaded.metadata.name, "New Name");
    }

    #[test]
    fn rename_collection_fails_without_wire_yaml() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(&wire_dir).unwrap();
        // No wire.yaml file

        let result = rename_collection(&wire_dir, "New Name");
        assert!(result.is_err());
    }

    #[test]
    fn rename_collection_preserves_other_metadata() {
        let dir = TempDir::new().unwrap();
        create_collection(dir.path(), "Original").unwrap();

        // Add an env file
        let wire_dir = dir.path().join(".wire");
        fs::write(
            wire_dir.join("envs/dev.yaml"),
            "name: Dev\nvariables:\n  url: http://localhost\n",
        )
        .unwrap();

        let renamed = rename_collection(&wire_dir, "Renamed").unwrap();
        assert_eq!(renamed.metadata.name, "Renamed");
        assert_eq!(renamed.metadata.version, 1);
        assert_eq!(renamed.environments.len(), 1);
    }

    #[test]
    fn load_collection_ignores_non_yaml_env_files() {
        let dir = TempDir::new().unwrap();
        let wire_dir = dir.path().join(".wire");
        fs::create_dir_all(wire_dir.join("envs")).unwrap();

        fs::write(
            wire_dir.join("envs/dev.yaml"),
            "name: Dev\nvariables:\n  url: http://localhost\n",
        )
        .unwrap();
        fs::write(wire_dir.join("envs/notes.txt"), "not an env").unwrap();

        let collection = load_collection(&wire_dir).unwrap();
        assert_eq!(collection.environments.len(), 1);
        assert!(collection.environments.contains_key("dev"));
    }
}
