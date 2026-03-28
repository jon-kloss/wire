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
