use crate::error::WireError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// A single entry in the request history log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub elapsed_ms: u64,
}

const HISTORY_FILENAME: &str = "history.jsonl";

/// Resolve the history file path.
/// Uses collection-scoped `.wire/history.jsonl` if a collection path is provided,
/// otherwise falls back to global `~/.wire/history.jsonl`.
pub fn resolve_history_path(collection_path: Option<&Path>) -> PathBuf {
    if let Some(col_path) = collection_path {
        col_path.join(HISTORY_FILENAME)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".wire")
            .join(HISTORY_FILENAME)
    }
}

/// Append a history entry to the JSONL file.
/// Creates parent directories and the file if they don't exist.
/// On Unix, creates the file with mode 0600.
pub fn save_entry(path: &Path, entry: &HistoryEntry) -> Result<(), WireError> {
    // Create parent directories
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Serialize as compact single-line JSON (NEVER pretty-print — breaks JSONL)
    let mut line = serde_json::to_string(entry)
        .map_err(|e| WireError::Other(format!("Failed to serialize history entry: {e}")))?;
    line.push('\n');

    // Open file in append mode, create if missing
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);

    // On Unix, set file permissions to 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts.open(path)?;
    file.write_all(line.as_bytes())?;

    Ok(())
}

/// Load history entries from a JSONL file.
/// Returns the most recent `limit` entries.
/// Returns `Ok(vec![])` for missing or empty files.
/// Skips unparseable lines (graceful degradation).
pub fn load_history(path: &Path, limit: usize) -> Result<Vec<HistoryEntry>, WireError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    let mut entries: Vec<HistoryEntry> = Vec::new();
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue, // skip I/O errors on individual lines
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<HistoryEntry>(trimmed) {
            Ok(entry) => entries.push(entry),
            Err(_) => continue, // skip malformed lines
        }
    }

    // Return the most recent `limit` entries
    if entries.len() > limit {
        entries = entries.split_off(entries.len() - limit);
    }

    Ok(entries)
}

/// Clear history by removing the file. Idempotent — succeeds even if file doesn't exist.
pub fn clear_history(path: &Path) -> Result<(), WireError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(WireError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_entry() -> HistoryEntry {
        HistoryEntry {
            timestamp: Utc::now(),
            name: "Test Request".to_string(),
            method: "GET".to_string(),
            url: "https://api.example.com/users".to_string(),
            status: 200,
            elapsed_ms: 42,
        }
    }

    #[test]
    fn save_and_load_single_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");

        let entry = sample_entry();
        save_entry(&path, &entry).unwrap();

        let loaded = load_history(&path, 50).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].method, "GET");
        assert_eq!(loaded[0].url, "https://api.example.com/users");
        assert_eq!(loaded[0].status, 200);
    }

    #[test]
    fn save_multiple_and_load_with_limit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");

        for i in 0..10 {
            let entry = HistoryEntry {
                timestamp: Utc::now(),
                name: format!("Request {i}"),
                method: "GET".to_string(),
                url: format!("https://example.com/{i}"),
                status: 200,
                elapsed_ms: i as u64,
            };
            save_entry(&path, &entry).unwrap();
        }

        // Load with limit 3 — should get the last 3
        let loaded = load_history(&path, 3).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].url, "https://example.com/7");
        assert_eq!(loaded[2].url, "https://example.com/9");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.jsonl");

        let loaded = load_history(&path, 50).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_empty_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.jsonl");
        fs::write(&path, "").unwrap();

        let loaded = load_history(&path, 50).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_skips_malformed_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");

        let good_entry = sample_entry();
        let good_json = serde_json::to_string(&good_entry).unwrap();

        let content = format!(
            "{good_json}\nthis is not json\n{good_json}\n{{\"broken: true}}\n{good_json}\n"
        );
        fs::write(&path, content).unwrap();

        let loaded = load_history(&path, 50).unwrap();
        assert_eq!(loaded.len(), 3); // only the 3 valid lines
    }

    #[test]
    fn clear_history_removes_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");

        save_entry(&path, &sample_entry()).unwrap();
        assert!(path.exists());

        clear_history(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn clear_history_idempotent_on_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.jsonl");

        // Should not error
        clear_history(&path).unwrap();
    }

    #[test]
    fn save_entry_creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("history.jsonl");

        save_entry(&path, &sample_entry()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn resolve_history_path_with_collection() {
        let col = Path::new("/tmp/myproject/.wire");
        let resolved = resolve_history_path(Some(col));
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/myproject/.wire/history.jsonl")
        );
    }

    #[test]
    fn resolve_history_path_global_fallback() {
        let resolved = resolve_history_path(None);
        assert!(resolved.to_string_lossy().contains(".wire"));
        assert!(resolved.to_string_lossy().ends_with("history.jsonl"));
    }

    #[test]
    fn serialized_entry_has_no_newlines() {
        let entry = HistoryEntry {
            timestamp: Utc::now(),
            name: "Test with\nnewline".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/path?q=hello\nworld".to_string(),
            status: 200,
            elapsed_ms: 10,
        };
        let json = serde_json::to_string(&entry).unwrap();
        // serde_json escapes newlines as \n in the JSON string, so the serialized
        // line itself should contain no raw newline characters
        assert!(
            !json.contains('\n'),
            "Serialized JSON must not contain raw newlines"
        );
    }

    #[test]
    fn forward_compatible_deserialization() {
        // Simulate an old entry missing the 'name' field
        let old_json = r#"{"timestamp":"2026-03-28T00:00:00Z","method":"GET","url":"https://example.com","status":200,"elapsed_ms":50}"#;
        let entry: HistoryEntry = serde_json::from_str(old_json).unwrap();
        assert_eq!(entry.name, ""); // default empty string
        assert_eq!(entry.method, "GET");
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");

        save_entry(&path, &sample_entry()).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "History file should have 0600 permissions");
    }
}
