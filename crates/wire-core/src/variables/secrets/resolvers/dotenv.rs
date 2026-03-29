use crate::error::WireError;
use std::path::Path;

/// Resolve a secret from a .env file.
/// Auto-discovers .env in project_dir, or falls back to current directory.
pub fn resolve(key: &str, project_dir: Option<&Path>) -> Result<String, WireError> {
    let dotenv_path = find_dotenv(project_dir)?;
    let content = std::fs::read_to_string(&dotenv_path).map_err(|e| {
        WireError::Other(format!(
            "Failed to read .env at {}: {e}",
            dotenv_path.display()
        ))
    })?;

    parse_dotenv(&content, key).ok_or_else(|| {
        WireError::Other(format!(
            "Secret not found: $dotenv:{key} — key '{key}' not found in {}",
            dotenv_path.display()
        ))
    })
}

/// Find the .env file, checking project_dir first, then current directory.
fn find_dotenv(project_dir: Option<&Path>) -> Result<std::path::PathBuf, WireError> {
    if let Some(dir) = project_dir {
        let path = dir.join(".env");
        if path.exists() {
            return Ok(path);
        }
    }

    // Fallback to current directory
    let cwd_path = std::path::PathBuf::from(".env");
    if cwd_path.exists() {
        return Ok(cwd_path);
    }

    Err(WireError::Other(format!(
        "No .env file found{}",
        project_dir
            .map(|d| format!(" in {} or current directory", d.display()))
            .unwrap_or_else(|| " in current directory".to_string())
    )))
}

/// Parse a .env file and extract a specific key's value.
fn parse_dotenv(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split on first '='
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k == key {
                let v = v.trim();
                // Strip surrounding quotes
                let v = if (v.starts_with('"') && v.ends_with('"'))
                    || (v.starts_with('\'') && v.ends_with('\''))
                {
                    &v[1..v.len() - 1]
                } else {
                    // Strip inline comments for unquoted values
                    v.split('#').next().unwrap_or(v).trim_end()
                };
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_key_value() {
        assert_eq!(parse_dotenv("KEY=value", "KEY"), Some("value".to_string()));
    }

    #[test]
    fn parse_with_spaces_around_equals() {
        assert_eq!(
            parse_dotenv("KEY = value", "KEY"),
            Some("value".to_string())
        );
    }

    #[test]
    fn parse_double_quoted() {
        assert_eq!(
            parse_dotenv("KEY=\"hello world\"", "KEY"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_single_quoted() {
        assert_eq!(
            parse_dotenv("KEY='hello world'", "KEY"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_skips_comments() {
        let content = "# comment\nKEY=value\n# another comment\n";
        assert_eq!(parse_dotenv(content, "KEY"), Some("value".to_string()));
    }

    #[test]
    fn parse_inline_comment() {
        assert_eq!(
            parse_dotenv("KEY=value # comment", "KEY"),
            Some("value".to_string())
        );
    }

    #[test]
    fn parse_missing_key() {
        assert_eq!(parse_dotenv("OTHER=value", "KEY"), None);
    }

    #[test]
    fn parse_empty_value() {
        assert_eq!(parse_dotenv("KEY=", "KEY"), Some("".to_string()));
    }

    #[test]
    fn parse_multiple_keys() {
        let content = "A=1\nB=2\nC=3\n";
        assert_eq!(parse_dotenv(content, "B"), Some("2".to_string()));
    }

    #[test]
    fn find_dotenv_in_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".env"), "X=1\n").unwrap();
        let path = find_dotenv(Some(dir.path())).unwrap();
        assert_eq!(path, dir.path().join(".env"));
    }

    #[test]
    fn find_dotenv_missing_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = find_dotenv(Some(dir.path()));
        assert!(result.is_err());
    }
}
