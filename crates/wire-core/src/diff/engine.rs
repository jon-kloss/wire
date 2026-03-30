use serde_json::Value;

use super::{DiffEntry, DiffKind};

/// Compare two JSON values structurally and return a list of differences.
///
/// Walks both trees recursively, reporting added, removed, and changed fields
/// with human-readable paths (e.g. "users[0].name").
pub fn structural_diff(old: &Value, new: &Value) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();
    diff_values(old, new, String::new(), &mut diffs);
    diffs
}

fn diff_values(old: &Value, new: &Value, path: String, diffs: &mut Vec<DiffEntry>) {
    if old == new {
        return;
    }

    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            // Check for removed and changed keys
            for (key, old_val) in old_map {
                let child_path = join_path(&path, key);
                match new_map.get(key) {
                    Some(new_val) => diff_values(old_val, new_val, child_path, diffs),
                    None => diffs.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Removed,
                        old: Some(old_val.clone()),
                        new: None,
                    }),
                }
            }
            // Check for added keys
            for (key, new_val) in new_map {
                if !old_map.contains_key(key) {
                    diffs.push(DiffEntry {
                        path: join_path(&path, key),
                        kind: DiffKind::Added,
                        old: None,
                        new: Some(new_val.clone()),
                    });
                }
            }
        }
        (Value::Array(old_arr), Value::Array(new_arr)) => {
            let max_len = old_arr.len().max(new_arr.len());
            for i in 0..max_len {
                let child_path = join_index(&path, i);
                match (old_arr.get(i), new_arr.get(i)) {
                    (Some(old_val), Some(new_val)) => {
                        diff_values(old_val, new_val, child_path, diffs);
                    }
                    (Some(old_val), None) => {
                        diffs.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Removed,
                            old: Some(old_val.clone()),
                            new: None,
                        });
                    }
                    (None, Some(new_val)) => {
                        diffs.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Added,
                            old: None,
                            new: Some(new_val.clone()),
                        });
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        // Different types or different primitive values
        _ => {
            diffs.push(DiffEntry {
                path,
                kind: DiffKind::Changed,
                old: Some(old.clone()),
                new: Some(new.clone()),
            });
        }
    }
}

fn join_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

fn join_index(parent: &str, index: usize) -> String {
    format!("{parent}[{index}]")
}
