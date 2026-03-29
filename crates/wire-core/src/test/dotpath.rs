/// Extract a value from a JSON object using dotpath notation.
///
/// Supports:
/// - `body.name` — object field access
/// - `body.users[0]` — array index access
/// - `body.users[0].email` — nested access
/// - `body.data[0].tags[1]` — multi-level array
pub fn extract(json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = json.clone();

    for segment in parse_segments(path) {
        match segment {
            Segment::Field(name) => {
                current = current.get(&name)?.clone();
            }
            Segment::Index(idx) => {
                current = current.get(idx)?.clone();
            }
        }
    }

    Some(current)
}

enum Segment {
    Field(String),
    Index(usize),
}

/// Parse a dotpath like "users[0].email" into segments.
fn parse_segments(path: &str) -> Vec<Segment> {
    let mut segments = Vec::new();

    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }

        // Check for array index: "users[0]"
        if let Some(bracket_pos) = part.find('[') {
            let field = &part[..bracket_pos];
            if !field.is_empty() {
                segments.push(Segment::Field(field.to_string()));
            }

            // Extract all indices: could be "items[0]" or just "[0]"
            let mut rest = &part[bracket_pos..];
            while let Some(start) = rest.find('[') {
                if let Some(end) = rest.find(']') {
                    if let Ok(idx) = rest[start + 1..end].parse::<usize>() {
                        segments.push(Segment::Index(idx));
                    }
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        } else {
            segments.push(Segment::Field(part.to_string()));
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_field() {
        let data = json!({"name": "Jon", "age": 30});
        assert_eq!(extract(&data, "name"), Some(json!("Jon")));
        assert_eq!(extract(&data, "age"), Some(json!(30)));
    }

    #[test]
    fn nested_field() {
        let data = json!({"user": {"name": "Jon", "address": {"city": "Portland"}}});
        assert_eq!(extract(&data, "user.name"), Some(json!("Jon")));
        assert_eq!(extract(&data, "user.address.city"), Some(json!("Portland")));
    }

    #[test]
    fn array_index() {
        let data = json!({"users": [{"name": "Alice"}, {"name": "Bob"}]});
        assert_eq!(extract(&data, "users[0].name"), Some(json!("Alice")));
        assert_eq!(extract(&data, "users[1].name"), Some(json!("Bob")));
    }

    #[test]
    fn top_level_array() {
        let data = json!([{"id": 1}, {"id": 2}]);
        assert_eq!(extract(&data, "[0].id"), Some(json!(1)));
        assert_eq!(extract(&data, "[1].id"), Some(json!(2)));
    }

    #[test]
    fn missing_field_returns_none() {
        let data = json!({"name": "Jon"});
        assert_eq!(extract(&data, "missing"), None);
        assert_eq!(extract(&data, "name.nested"), None);
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let data = json!({"items": [1, 2, 3]});
        assert_eq!(extract(&data, "items[99]"), None);
    }

    #[test]
    fn whole_object() {
        let data = json!({"users": [1, 2]});
        assert_eq!(extract(&data, "users"), Some(json!([1, 2])));
    }

    #[test]
    fn deeply_nested() {
        let data = json!({"a": {"b": {"c": {"d": 42}}}});
        assert_eq!(extract(&data, "a.b.c.d"), Some(json!(42)));
    }
}
