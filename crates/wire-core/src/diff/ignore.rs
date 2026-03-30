use super::DiffEntry;

/// A rule that filters diff entries by path pattern.
///
/// Supports:
/// - Exact paths: "body.timestamp"
/// - Wildcard array indices: "body.users[*].id" matches "body.users[0].id", "body.users[1].id", etc.
#[derive(Debug, Clone, PartialEq)]
pub struct IgnoreRule {
    pattern: String,
}

impl IgnoreRule {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
        }
    }

    /// Check if this rule matches the given diff path.
    pub fn matches(&self, path: &str) -> bool {
        if !self.pattern.contains("[*]") {
            return self.pattern == path;
        }
        // Wildcard matching: split pattern and path on [*] vs [N]
        let pattern_parts: Vec<&str> = self.pattern.split("[*]").collect();
        let mut remaining = path;

        for (i, part) in pattern_parts.iter().enumerate() {
            if i > 0 {
                // Skip past the concrete index like [0], [1], etc.
                if let Some(bracket_start) = remaining.find('[') {
                    if let Some(bracket_end) = remaining[bracket_start..].find(']') {
                        remaining = &remaining[bracket_start + bracket_end + 1..];
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        }
        remaining.is_empty()
    }
}

/// Filter out diff entries that match any of the ignore rules.
pub fn filter_diffs(diffs: Vec<DiffEntry>, rules: &[IgnoreRule]) -> Vec<DiffEntry> {
    if rules.is_empty() {
        return diffs;
    }
    diffs
        .into_iter()
        .filter(|d| !rules.iter().any(|r| r.matches(&d.path)))
        .collect()
}

/// Parse a list of pattern strings into ignore rules.
pub fn parse_ignore_rules(patterns: &[String]) -> Vec<IgnoreRule> {
    patterns.iter().map(|p| IgnoreRule::new(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffEntry, DiffKind};
    use serde_json::json;

    fn entry(path: &str, kind: DiffKind) -> DiffEntry {
        DiffEntry {
            path: path.to_string(),
            kind,
            old: Some(json!("old")),
            new: Some(json!("new")),
        }
    }

    #[test]
    fn exact_match() {
        let rule = IgnoreRule::new("body.timestamp");
        assert!(rule.matches("body.timestamp"));
        assert!(!rule.matches("body.name"));
        assert!(!rule.matches("body.timestamp.nested"));
    }

    #[test]
    fn wildcard_array_match() {
        let rule = IgnoreRule::new("body.users[*].id");
        assert!(rule.matches("body.users[0].id"));
        assert!(rule.matches("body.users[1].id"));
        assert!(rule.matches("body.users[99].id"));
        assert!(!rule.matches("body.users[0].name"));
        assert!(!rule.matches("body.items[0].id"));
    }

    #[test]
    fn multiple_wildcards() {
        let rule = IgnoreRule::new("data[*].items[*].ts");
        assert!(rule.matches("data[0].items[0].ts"));
        assert!(rule.matches("data[5].items[3].ts"));
        assert!(!rule.matches("data[0].items[0].name"));
    }

    #[test]
    fn filter_removes_matching_entries() {
        let diffs = vec![
            entry("body.name", DiffKind::Changed),
            entry("body.timestamp", DiffKind::Changed),
            entry("body.request_id", DiffKind::Changed),
        ];
        let rules = vec![
            IgnoreRule::new("body.timestamp"),
            IgnoreRule::new("body.request_id"),
        ];
        let filtered = filter_diffs(diffs, &rules);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "body.name");
    }

    #[test]
    fn filter_with_wildcard_rules() {
        let diffs = vec![
            entry("body.users[0].id", DiffKind::Changed),
            entry("body.users[1].id", DiffKind::Changed),
            entry("body.users[0].name", DiffKind::Changed),
        ];
        let rules = vec![IgnoreRule::new("body.users[*].id")];
        let filtered = filter_diffs(diffs, &rules);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "body.users[0].name");
    }

    #[test]
    fn empty_rules_pass_everything() {
        let diffs = vec![entry("a", DiffKind::Added), entry("b", DiffKind::Removed)];
        let filtered = filter_diffs(diffs, &[]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn non_matching_rules_pass_everything() {
        let diffs = vec![entry("body.name", DiffKind::Changed)];
        let rules = vec![IgnoreRule::new("body.other")];
        let filtered = filter_diffs(diffs, &rules);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn parse_ignore_rules_from_strings() {
        let patterns = vec!["body.timestamp".to_string(), "body.users[*].id".to_string()];
        let rules = parse_ignore_rules(&patterns);
        assert_eq!(rules.len(), 2);
        assert!(rules[0].matches("body.timestamp"));
        assert!(rules[1].matches("body.users[0].id"));
    }
}
