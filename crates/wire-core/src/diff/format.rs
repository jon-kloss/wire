use super::{DiffEntry, DiffKind};

/// Format diff entries as plain text for CLI output.
pub fn format_diff(diffs: &[DiffEntry]) -> String {
    if diffs.is_empty() {
        return "No differences found".to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    for entry in diffs {
        let path = if entry.path.is_empty() {
            "(root)".to_string()
        } else {
            entry.path.clone()
        };
        match entry.kind {
            DiffKind::Added => {
                let val = format_value(entry.new.as_ref());
                lines.push(format!("+ {path}: {val}"));
            }
            DiffKind::Removed => {
                let val = format_value(entry.old.as_ref());
                lines.push(format!("- {path}: {val}"));
            }
            DiffKind::Changed => {
                let old = format_value(entry.old.as_ref());
                let new = format_value(entry.new.as_ref());
                lines.push(format!("~ {path}: {old} \u{2192} {new}"));
            }
        }
    }

    let added = diffs.iter().filter(|d| d.kind == DiffKind::Added).count();
    let removed = diffs.iter().filter(|d| d.kind == DiffKind::Removed).count();
    let changed = diffs.iter().filter(|d| d.kind == DiffKind::Changed).count();

    lines.push(String::new());
    lines.push(format!(
        "{added} added, {removed} removed, {changed} changed"
    ));
    lines.join("\n")
}

fn format_value(val: Option<&serde_json::Value>) -> String {
    match val {
        None => "(none)".to_string(),
        Some(serde_json::Value::String(s)) => format!("\"{s}\""),
        Some(serde_json::Value::Null) => "null".to_string(),
        Some(v) => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(
        path: &str,
        kind: DiffKind,
        old: Option<serde_json::Value>,
        new: Option<serde_json::Value>,
    ) -> DiffEntry {
        DiffEntry {
            path: path.to_string(),
            kind,
            old,
            new,
        }
    }

    #[test]
    fn empty_diff_message() {
        assert_eq!(format_diff(&[]), "No differences found");
    }

    #[test]
    fn added_entry_format() {
        let diffs = vec![entry("body.age", DiffKind::Added, None, Some(json!(30)))];
        let output = format_diff(&diffs);
        assert!(output.contains("+ body.age: 30"));
        assert!(output.contains("1 added, 0 removed, 0 changed"));
    }

    #[test]
    fn removed_entry_format() {
        let diffs = vec![entry(
            "body.old",
            DiffKind::Removed,
            Some(json!("gone")),
            None,
        )];
        let output = format_diff(&diffs);
        assert!(output.contains("- body.old: \"gone\""));
        assert!(output.contains("0 added, 1 removed, 0 changed"));
    }

    #[test]
    fn changed_entry_format() {
        let diffs = vec![entry(
            "body.name",
            DiffKind::Changed,
            Some(json!("Alice")),
            Some(json!("Bob")),
        )];
        let output = format_diff(&diffs);
        assert!(output.contains("~ body.name: \"Alice\" \u{2192} \"Bob\""));
        assert!(output.contains("0 added, 0 removed, 1 changed"));
    }

    #[test]
    fn mixed_diffs_summary() {
        let diffs = vec![
            entry("a", DiffKind::Added, None, Some(json!(1))),
            entry("b", DiffKind::Removed, Some(json!(2)), None),
            entry("c", DiffKind::Changed, Some(json!(3)), Some(json!(4))),
            entry("d", DiffKind::Changed, Some(json!(5)), Some(json!(6))),
        ];
        let output = format_diff(&diffs);
        assert!(output.contains("1 added, 1 removed, 2 changed"));
    }

    #[test]
    fn null_value_formatted() {
        let diffs = vec![entry(
            "body.val",
            DiffKind::Changed,
            Some(json!(null)),
            Some(json!(42)),
        )];
        let output = format_diff(&diffs);
        assert!(output.contains("~ body.val: null \u{2192} 42"));
    }
}
