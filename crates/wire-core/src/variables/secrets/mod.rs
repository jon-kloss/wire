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

/// Check if a value is a secret reference without resolving it.
pub fn is_secret(value: &str) -> bool {
    value.starts_with("$env:")
        || value.starts_with("$dotenv:")
        || value.starts_with("$aws:")
        || value.starts_with("$vault:")
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
}
