use crate::error::WireError;
use std::path::Path;

mod resolvers;

/// Supported secret sources.
#[derive(Debug, Clone, PartialEq)]
pub enum SecretSource {
    /// $env:VAR_NAME — read from process environment
    Env,
    /// $dotenv:VAR_NAME — read from .env file
    Dotenv,
    /// $aws:secret-name/key — read from AWS Secrets Manager
    Aws,
    /// $vault:secret/path#key — read from HashiCorp Vault
    Vault,
}

/// A parsed secret reference.
#[derive(Debug, Clone, PartialEq)]
pub struct SecretRef {
    pub source: SecretSource,
    pub key: String,
}

/// Check if a variable value is a secret reference (starts with $).
/// Returns None for regular values.
pub fn parse_secret_ref(value: &str) -> Option<SecretRef> {
    let prefixes: &[(&str, SecretSource)] = &[
        ("$env:", SecretSource::Env),
        ("$dotenv:", SecretSource::Dotenv),
        ("$aws:", SecretSource::Aws),
        ("$vault:", SecretSource::Vault),
    ];

    for (prefix, source) in prefixes {
        if let Some(key) = value.strip_prefix(prefix) {
            return Some(SecretRef {
                source: source.clone(),
                key: key.to_string(),
            });
        }
    }
    None
}

/// Resolve a secret reference to its actual value.
/// `project_dir` is used for .env file discovery.
pub fn resolve_secret(secret: &SecretRef, project_dir: Option<&Path>) -> Result<String, WireError> {
    match secret.source {
        SecretSource::Env => resolvers::env::resolve(&secret.key),
        SecretSource::Dotenv => resolvers::dotenv::resolve(&secret.key, project_dir),
        SecretSource::Aws => resolvers::aws::resolve(&secret.key),
        SecretSource::Vault => resolvers::vault::resolve(&secret.key),
    }
}

/// Result of checking a single secret reference.
#[derive(Debug, Clone)]
pub struct SecretCheckResult {
    pub env_name: String,
    pub var_name: String,
    pub source: String,
    pub key: String,
    pub resolved: bool,
    pub error: Option<String>,
}

/// Check all secret references in a collection's environments.
/// Returns a list of check results (both passed and failed).
pub fn check_collection_secrets(
    environments: &std::collections::HashMap<String, crate::collection::Environment>,
    project_dir: Option<&Path>,
) -> Vec<SecretCheckResult> {
    let mut results = Vec::new();

    let mut env_names: Vec<&String> = environments.keys().collect();
    env_names.sort();

    for env_name in env_names {
        let env = &environments[env_name];
        let mut var_names: Vec<&String> = env.variables.keys().collect();
        var_names.sort();

        for var_name in var_names {
            let value = &env.variables[var_name];
            if let Some(secret_ref) = parse_secret_ref(value) {
                let source = format!("{:?}", secret_ref.source).to_lowercase();
                let key = secret_ref.key.clone();
                match resolve_secret(&secret_ref, project_dir) {
                    Ok(_) => results.push(SecretCheckResult {
                        env_name: env_name.clone(),
                        var_name: var_name.clone(),
                        source,
                        key,
                        resolved: true,
                        error: None,
                    }),
                    Err(e) => results.push(SecretCheckResult {
                        env_name: env_name.clone(),
                        var_name: var_name.clone(),
                        source,
                        key,
                        resolved: false,
                        error: Some(e.to_string()),
                    }),
                }
            }
        }
    }

    results
}

/// Check if a value is a secret reference without resolving it.
pub fn is_secret(value: &str) -> bool {
    value.starts_with("$env:")
        || value.starts_with("$dotenv:")
        || value.starts_with("$aws:")
        || value.starts_with("$vault:")
}

/// Mask a secret value for display.
pub fn mask_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let visible = std::cmp::min(4, value.len() / 4);
    if visible == 0 || value.len() < 8 {
        "*".repeat(value.len().min(8))
    } else {
        format!("{}{}", &value[..visible], "*".repeat(8))
    }
}

/// Find which variable names in a scope have secret values.
/// Returns the set of variable names whose values start with a secret prefix.
pub fn find_secret_var_names(
    environments: &std::collections::HashMap<String, crate::collection::Environment>,
    active_env: Option<&str>,
) -> std::collections::HashSet<String> {
    let mut secret_names = std::collections::HashSet::new();
    if let Some(env_key) = active_env {
        if let Some(env) = environments.get(env_key) {
            for (name, value) in &env.variables {
                if is_secret(value) {
                    secret_names.insert(name.clone());
                }
            }
        }
    }
    secret_names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_ref() {
        let r = parse_secret_ref("$env:API_KEY").unwrap();
        assert_eq!(r.source, SecretSource::Env);
        assert_eq!(r.key, "API_KEY");
    }

    #[test]
    fn parse_dotenv_ref() {
        let r = parse_secret_ref("$dotenv:DB_PASSWORD").unwrap();
        assert_eq!(r.source, SecretSource::Dotenv);
        assert_eq!(r.key, "DB_PASSWORD");
    }

    #[test]
    fn parse_aws_ref() {
        let r = parse_secret_ref("$aws:prod/stripe/secret_key").unwrap();
        assert_eq!(r.source, SecretSource::Aws);
        assert_eq!(r.key, "prod/stripe/secret_key");
    }

    #[test]
    fn parse_vault_ref() {
        let r = parse_secret_ref("$vault:secret/data/app#token").unwrap();
        assert_eq!(r.source, SecretSource::Vault);
        assert_eq!(r.key, "secret/data/app#token");
    }

    #[test]
    fn parse_regular_value_returns_none() {
        assert!(parse_secret_ref("https://api.example.com").is_none());
        assert!(parse_secret_ref("plain-value").is_none());
        assert!(parse_secret_ref("").is_none());
    }

    #[test]
    fn parse_dollar_without_known_prefix_returns_none() {
        assert!(parse_secret_ref("$unknown:foo").is_none());
        assert!(parse_secret_ref("$").is_none());
    }

    #[test]
    fn is_secret_detects_all_prefixes() {
        assert!(is_secret("$env:X"));
        assert!(is_secret("$dotenv:X"));
        assert!(is_secret("$aws:X"));
        assert!(is_secret("$vault:X"));
        assert!(!is_secret("plain"));
        assert!(!is_secret("$unknown:X"));
    }

    #[test]
    fn resolve_env_from_process() {
        std::env::set_var("WIRE_TEST_SECRET", "test-value-123");
        let r = SecretRef {
            source: SecretSource::Env,
            key: "WIRE_TEST_SECRET".to_string(),
        };
        let value = resolve_secret(&r, None).unwrap();
        assert_eq!(value, "test-value-123");
        std::env::remove_var("WIRE_TEST_SECRET");
    }

    #[test]
    fn resolve_env_missing_fails() {
        let r = SecretRef {
            source: SecretSource::Env,
            key: "WIRE_NONEXISTENT_VAR_12345".to_string(),
        };
        let err = resolve_secret(&r, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("WIRE_NONEXISTENT_VAR_12345"));
        assert!(msg.contains("env"));
    }

    #[test]
    fn resolve_dotenv_from_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "DB_HOST=localhost\nDB_PORT=5432\n# comment\nAPI_KEY=secret-abc\n",
        )
        .unwrap();

        let r = SecretRef {
            source: SecretSource::Dotenv,
            key: "API_KEY".to_string(),
        };
        let value = resolve_secret(&r, Some(dir.path())).unwrap();
        assert_eq!(value, "secret-abc");
    }

    #[test]
    fn resolve_dotenv_missing_key_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".env"), "EXISTING=yes\n").unwrap();

        let r = SecretRef {
            source: SecretSource::Dotenv,
            key: "MISSING_KEY".to_string(),
        };
        let err = resolve_secret(&r, Some(dir.path())).unwrap_err();
        assert!(err.to_string().contains("MISSING_KEY"));
    }

    #[test]
    fn resolve_dotenv_no_file_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        // No .env file

        let r = SecretRef {
            source: SecretSource::Dotenv,
            key: "ANYTHING".to_string(),
        };
        let err = resolve_secret(&r, Some(dir.path())).unwrap_err();
        assert!(err.to_string().contains(".env"));
    }

    #[test]
    fn resolve_dotenv_handles_quoted_values() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "SINGLE='single-quoted'\nDOUBLE=\"double-quoted\"\n",
        )
        .unwrap();

        let r1 = SecretRef {
            source: SecretSource::Dotenv,
            key: "SINGLE".to_string(),
        };
        assert_eq!(
            resolve_secret(&r1, Some(dir.path())).unwrap(),
            "single-quoted"
        );

        let r2 = SecretRef {
            source: SecretSource::Dotenv,
            key: "DOUBLE".to_string(),
        };
        assert_eq!(
            resolve_secret(&r2, Some(dir.path())).unwrap(),
            "double-quoted"
        );
    }

    #[test]
    fn mask_short_value() {
        assert_eq!(mask_value("abc"), "***");
        assert_eq!(mask_value("abcdef"), "******");
    }

    #[test]
    fn mask_long_value() {
        let masked = mask_value("super-secret-token-12345");
        // First few chars visible, rest masked
        assert!(masked.starts_with("supe"));
        assert!(masked.contains("*"));
        assert_ne!(masked, "super-secret-token-12345");
    }

    #[test]
    fn mask_empty_value() {
        assert_eq!(mask_value(""), "");
    }

    #[test]
    fn find_secret_names_in_env() {
        let mut envs = std::collections::HashMap::new();
        let mut vars = std::collections::HashMap::new();
        vars.insert("base_url".to_string(), "https://example.com".to_string());
        vars.insert("api_key".to_string(), "$env:API_KEY".to_string());
        vars.insert("db_pass".to_string(), "$dotenv:DB_PASS".to_string());
        envs.insert(
            "dev".to_string(),
            crate::collection::Environment {
                name: "Dev".to_string(),
                variables: vars,
            },
        );

        let names = find_secret_var_names(&envs, Some("dev"));
        assert!(names.contains("api_key"));
        assert!(names.contains("db_pass"));
        assert!(!names.contains("base_url"));
        assert_eq!(names.len(), 2);
    }
}
