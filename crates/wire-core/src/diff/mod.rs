mod engine;
pub mod format;
pub mod ignore;

pub use engine::structural_diff;

/// The kind of difference found between two JSON values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Changed,
}

/// A single difference at a specific JSON path.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffEntry {
    /// Human-readable path, e.g. "users[0].name"
    pub path: String,
    pub kind: DiffKind,
    pub old: Option<serde_json::Value>,
    pub new: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_values_produce_empty_diff() {
        let a = json!({"name": "Alice", "age": 30});
        let b = json!({"name": "Alice", "age": 30});
        assert!(structural_diff(&a, &b).is_empty());
    }

    #[test]
    fn added_field_detected() {
        let a = json!({"name": "Alice"});
        let b = json!({"name": "Alice", "age": 30});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "age");
        assert_eq!(diffs[0].kind, DiffKind::Added);
        assert_eq!(diffs[0].old, None);
        assert_eq!(diffs[0].new, Some(json!(30)));
    }

    #[test]
    fn removed_field_detected() {
        let a = json!({"name": "Alice", "age": 30});
        let b = json!({"name": "Alice"});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "age");
        assert_eq!(diffs[0].kind, DiffKind::Removed);
        assert_eq!(diffs[0].old, Some(json!(30)));
        assert_eq!(diffs[0].new, None);
    }

    #[test]
    fn changed_value_detected() {
        let a = json!({"name": "Alice"});
        let b = json!({"name": "Bob"});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "name");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
        assert_eq!(diffs[0].old, Some(json!("Alice")));
        assert_eq!(diffs[0].new, Some(json!("Bob")));
    }

    #[test]
    fn nested_object_diff() {
        let a = json!({"user": {"name": "Alice", "email": "a@test.com"}});
        let b = json!({"user": {"name": "Bob", "email": "a@test.com"}});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "user.name");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }

    #[test]
    fn deeply_nested_diff() {
        let a = json!({"a": {"b": {"c": {"d": 1}}}});
        let b = json!({"a": {"b": {"c": {"d": 2}}}});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "a.b.c.d");
    }

    #[test]
    fn array_element_changed() {
        let a = json!({"items": ["apple", "banana"]});
        let b = json!({"items": ["apple", "cherry"]});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "items[1]");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }

    #[test]
    fn array_element_added() {
        let a = json!({"items": ["apple"]});
        let b = json!({"items": ["apple", "banana"]});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "items[1]");
        assert_eq!(diffs[0].kind, DiffKind::Added);
    }

    #[test]
    fn array_element_removed() {
        let a = json!({"items": ["apple", "banana"]});
        let b = json!({"items": ["apple"]});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "items[1]");
        assert_eq!(diffs[0].kind, DiffKind::Removed);
    }

    #[test]
    fn array_of_objects() {
        let a = json!({"users": [{"name": "Alice"}, {"name": "Bob"}]});
        let b = json!({"users": [{"name": "Alice"}, {"name": "Carol"}]});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "users[1].name");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }

    #[test]
    fn type_change_object_to_string() {
        let a = json!({"data": {"nested": true}});
        let b = json!({"data": "flat"});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "data");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }

    #[test]
    fn null_handling() {
        let a = json!({"value": null});
        let b = json!({"value": 42});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, DiffKind::Changed);
        assert_eq!(diffs[0].old, Some(json!(null)));
        assert_eq!(diffs[0].new, Some(json!(42)));
    }

    #[test]
    fn empty_objects_are_equal() {
        let a = json!({});
        let b = json!({});
        assert!(structural_diff(&a, &b).is_empty());
    }

    #[test]
    fn empty_arrays_are_equal() {
        let a = json!({"items": []});
        let b = json!({"items": []});
        assert!(structural_diff(&a, &b).is_empty());
    }

    #[test]
    fn multiple_diffs_at_same_level() {
        let a = json!({"a": 1, "b": 2, "c": 3});
        let b = json!({"a": 1, "b": 99, "c": 3, "d": 4});
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 2);
        let paths: Vec<&str> = diffs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"b"));
        assert!(paths.contains(&"d"));
    }

    #[test]
    fn top_level_primitives() {
        let a = json!("hello");
        let b = json!("world");
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }

    #[test]
    fn top_level_arrays() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 2, 4]);
        let diffs = structural_diff(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "[2]");
        assert_eq!(diffs[0].kind, DiffKind::Changed);
    }
}
