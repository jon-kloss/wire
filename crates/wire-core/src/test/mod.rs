pub mod dotpath;
pub mod runner;

use crate::http::WireResponse;
use serde::{Deserialize, Serialize};

/// A single test assertion defined in a .wire.yaml file.
///
/// YAML syntax:
/// ```yaml
/// tests:
///   - field: status
///     equals: 200
///   - field: body.name
///     contains: "Jon"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Assertion {
    /// The field to test: status, elapsed_ms, body.path, header.name
    pub field: String,

    // Comparison operators (only one should be set per assertion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_equals: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_with: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_with: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub less_than: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub greater_than: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_array: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_object: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_string: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_number: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_matches: Option<String>,
}

/// Result of evaluating a single assertion.
#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub field: String,
    pub operator: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

/// Evaluate all assertions against a response.
pub fn evaluate_assertions(assertions: &[Assertion], response: &WireResponse) -> Vec<TestResult> {
    assertions
        .iter()
        .map(|a| evaluate_one(a, response))
        .collect()
}

fn resolve_field(field: &str, response: &WireResponse) -> Option<serde_json::Value> {
    match field {
        "status" => Some(serde_json::Value::Number(response.status.into())),
        "elapsed_ms" => {
            let ms = response.elapsed.as_millis() as u64;
            Some(serde_json::Value::Number(ms.into()))
        }
        _ if field.starts_with("body.") => {
            let path = &field[5..];
            let body_json: serde_json::Value = serde_json::from_str(&response.body).ok()?;
            dotpath::extract(&body_json, path)
        }
        "body" => serde_json::from_str(&response.body).ok(),
        _ if field.starts_with("header.") => {
            let header_name = &field[7..];
            // Case-insensitive header lookup
            let value = response
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(header_name))
                .map(|(_, v)| v.clone())?;
            Some(serde_json::Value::String(value))
        }
        _ => None,
    }
}

fn evaluate_one(assertion: &Assertion, response: &WireResponse) -> TestResult {
    let resolved = resolve_field(&assertion.field, response);

    // Handle body_contains and body_matches (operate on raw body string, not a field)
    if let Some(ref needle) = assertion.body_contains {
        return TestResult {
            field: assertion.field.clone(),
            operator: "body_contains".to_string(),
            passed: response.body.contains(needle.as_str()),
            expected: needle.clone(),
            actual: truncate(&response.body, 100),
        };
    }

    if let Some(ref pattern) = assertion.body_matches {
        let passed = regex::Regex::new(pattern)
            .map(|re| re.is_match(&response.body))
            .unwrap_or(false);
        return TestResult {
            field: assertion.field.clone(),
            operator: "body_matches".to_string(),
            passed,
            expected: pattern.clone(),
            actual: truncate(&response.body, 100),
        };
    }

    // Handle exists operator
    if let Some(should_exist) = assertion.exists {
        let does_exist = resolved.is_some();
        return TestResult {
            field: assertion.field.clone(),
            operator: "exists".to_string(),
            passed: does_exist == should_exist,
            expected: should_exist.to_string(),
            actual: does_exist.to_string(),
        };
    }

    // For all other operators, we need a resolved value
    let value = match resolved {
        Some(v) => v,
        None => {
            return TestResult {
                field: assertion.field.clone(),
                operator: "resolve".to_string(),
                passed: false,
                expected: "field exists".to_string(),
                actual: "field not found".to_string(),
            };
        }
    };

    if let Some(ref expected) = assertion.equals {
        return eval_equals(&assertion.field, &value, expected);
    }
    if let Some(ref expected) = assertion.not_equals {
        let mut result = eval_equals(&assertion.field, &value, expected);
        result.operator = "not_equals".to_string();
        result.passed = !result.passed;
        return result;
    }
    if let Some(ref needle) = assertion.contains {
        let actual_str = value_to_string(&value);
        return TestResult {
            field: assertion.field.clone(),
            operator: "contains".to_string(),
            passed: actual_str.contains(needle.as_str()),
            expected: needle.clone(),
            actual: actual_str,
        };
    }
    if let Some(ref prefix) = assertion.starts_with {
        let actual_str = value_to_string(&value);
        return TestResult {
            field: assertion.field.clone(),
            operator: "starts_with".to_string(),
            passed: actual_str.starts_with(prefix.as_str()),
            expected: prefix.clone(),
            actual: actual_str,
        };
    }
    if let Some(ref suffix) = assertion.ends_with {
        let actual_str = value_to_string(&value);
        return TestResult {
            field: assertion.field.clone(),
            operator: "ends_with".to_string(),
            passed: actual_str.ends_with(suffix.as_str()),
            expected: suffix.clone(),
            actual: actual_str,
        };
    }
    if let Some(threshold) = assertion.less_than {
        let actual_num = value_to_f64(&value);
        return TestResult {
            field: assertion.field.clone(),
            operator: "less_than".to_string(),
            passed: actual_num.map(|n| n < threshold).unwrap_or(false),
            expected: threshold.to_string(),
            actual: actual_num
                .map(|n| n.to_string())
                .unwrap_or_else(|| value_to_string(&value)),
        };
    }
    if let Some(threshold) = assertion.greater_than {
        let actual_num = value_to_f64(&value);
        return TestResult {
            field: assertion.field.clone(),
            operator: "greater_than".to_string(),
            passed: actual_num.map(|n| n > threshold).unwrap_or(false),
            expected: threshold.to_string(),
            actual: actual_num
                .map(|n| n.to_string())
                .unwrap_or_else(|| value_to_string(&value)),
        };
    }
    if let Some(expected) = assertion.is_array {
        let is = value.is_array();
        return TestResult {
            field: assertion.field.clone(),
            operator: "is_array".to_string(),
            passed: is == expected,
            expected: expected.to_string(),
            actual: is.to_string(),
        };
    }
    if let Some(expected) = assertion.is_object {
        let is = value.is_object();
        return TestResult {
            field: assertion.field.clone(),
            operator: "is_object".to_string(),
            passed: is == expected,
            expected: expected.to_string(),
            actual: is.to_string(),
        };
    }
    if let Some(expected) = assertion.is_string {
        let is = value.is_string();
        return TestResult {
            field: assertion.field.clone(),
            operator: "is_string".to_string(),
            passed: is == expected,
            expected: expected.to_string(),
            actual: is.to_string(),
        };
    }
    if let Some(expected) = assertion.is_number {
        let is = value.is_number();
        return TestResult {
            field: assertion.field.clone(),
            operator: "is_number".to_string(),
            passed: is == expected,
            expected: expected.to_string(),
            actual: is.to_string(),
        };
    }

    TestResult {
        field: assertion.field.clone(),
        operator: "unknown".to_string(),
        passed: false,
        expected: "valid operator".to_string(),
        actual: "no operator specified".to_string(),
    }
}

fn eval_equals(
    field: &str,
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> TestResult {
    // Compare with type coercion: if expected is a number and actual is a number, compare numerically
    let passed = if actual == expected {
        true
    } else {
        // Try numeric comparison
        match (value_to_f64(actual), value_to_f64(expected)) {
            (Some(a), Some(e)) => (a - e).abs() < f64::EPSILON,
            _ => {
                // Try string comparison
                value_to_string(actual) == value_to_string(expected)
            }
        }
    };

    TestResult {
        field: field.to_string(),
        operator: "equals".to_string(),
        passed,
        expected: value_to_string(expected),
        actual: value_to_string(actual),
    }
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn value_to_f64(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid char boundary at or before max
        let end = s.floor_char_boundary(max);
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    fn mock_response(status: u16, body: &str) -> WireResponse {
        WireResponse {
            status,
            status_text: "OK".to_string(),
            headers: {
                let mut h = HashMap::new();
                h.insert("content-type".to_string(), "application/json".to_string());
                h.insert("x-request-id".to_string(), "abc123".to_string());
                h
            },
            body: body.to_string(),
            elapsed: Duration::from_millis(42),
            size_bytes: body.len(),
        }
    }

    fn make_assertion(field: &str) -> Assertion {
        Assertion {
            field: field.to_string(),
            equals: None,
            not_equals: None,
            contains: None,
            starts_with: None,
            ends_with: None,
            less_than: None,
            greater_than: None,
            is_array: None,
            is_object: None,
            is_string: None,
            is_number: None,
            exists: None,
            body_contains: None,
            body_matches: None,
        }
    }

    #[test]
    fn status_equals() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("status");
        a.equals = Some(serde_json::json!(200));
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn status_not_equals() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("status");
        a.not_equals = Some(serde_json::json!(404));
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn status_equals_fails() {
        let resp = mock_response(404, "{}");
        let mut a = make_assertion("status");
        a.equals = Some(serde_json::json!(200));
        let results = evaluate_assertions(&[a], &resp);
        assert!(!results[0].passed);
        assert_eq!(results[0].expected, "200");
        assert_eq!(results[0].actual, "404");
    }

    #[test]
    fn body_field_equals() {
        let resp = mock_response(200, r#"{"name": "Jon", "age": 30}"#);
        let mut a = make_assertion("body.name");
        a.equals = Some(serde_json::json!("Jon"));
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_nested_field() {
        let resp = mock_response(200, r#"{"user": {"email": "jon@example.com"}}"#);
        let mut a = make_assertion("body.user.email");
        a.equals = Some(serde_json::json!("jon@example.com"));
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_array_index() {
        let resp = mock_response(200, r#"{"items": [{"id": 1}, {"id": 2}]}"#);
        let mut a = make_assertion("body.items[0].id");
        a.equals = Some(serde_json::json!(1));
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn header_contains() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("header.content-type");
        a.contains = Some("application/json".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn header_case_insensitive() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("header.Content-Type");
        a.contains = Some("json".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn elapsed_ms_less_than() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("elapsed_ms");
        a.less_than = Some(500.0);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn elapsed_ms_greater_than() {
        let resp = mock_response(200, "{}");
        let mut a = make_assertion("elapsed_ms");
        a.greater_than = Some(10.0);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_is_array() {
        let resp = mock_response(200, r#"{"items": [1, 2, 3]}"#);
        let mut a = make_assertion("body.items");
        a.is_array = Some(true);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_is_object() {
        let resp = mock_response(200, r#"{"user": {"name": "Jon"}}"#);
        let mut a = make_assertion("body.user");
        a.is_object = Some(true);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_is_string() {
        let resp = mock_response(200, r#"{"name": "Jon"}"#);
        let mut a = make_assertion("body.name");
        a.is_string = Some(true);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_is_number() {
        let resp = mock_response(200, r#"{"count": 42}"#);
        let mut a = make_assertion("body.count");
        a.is_number = Some(true);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn field_exists() {
        let resp = mock_response(200, r#"{"name": "Jon"}"#);
        let mut a = make_assertion("body.name");
        a.exists = Some(true);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn field_not_exists() {
        let resp = mock_response(200, r#"{"name": "Jon"}"#);
        let mut a = make_assertion("body.missing");
        a.exists = Some(false);
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_contains_string() {
        let resp = mock_response(200, r#"{"message": "Hello World"}"#);
        let mut a = make_assertion("body");
        a.body_contains = Some("Hello".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn body_matches_regex() {
        let resp = mock_response(200, r#"{"id": "abc-123-def"}"#);
        let mut a = make_assertion("body");
        a.body_matches = Some(r"\w{3}-\d{3}-\w{3}".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn starts_with_operator() {
        let resp = mock_response(200, r#"{"url": "https://example.com/api"}"#);
        let mut a = make_assertion("body.url");
        a.starts_with = Some("https://".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn ends_with_operator() {
        let resp = mock_response(200, r#"{"file": "report.pdf"}"#);
        let mut a = make_assertion("body.file");
        a.ends_with = Some(".pdf".to_string());
        let results = evaluate_assertions(&[a], &resp);
        assert!(results[0].passed);
    }

    #[test]
    fn missing_field_fails() {
        let resp = mock_response(200, r#"{"name": "Jon"}"#);
        let mut a = make_assertion("body.nonexistent");
        a.equals = Some(serde_json::json!("value"));
        let results = evaluate_assertions(&[a], &resp);
        assert!(!results[0].passed);
        assert_eq!(results[0].actual, "field not found");
    }

    #[test]
    fn multiple_assertions() {
        let resp = mock_response(200, r#"{"name": "Jon", "items": [1, 2]}"#);
        let assertions = vec![
            {
                let mut a = make_assertion("status");
                a.equals = Some(serde_json::json!(200));
                a
            },
            {
                let mut a = make_assertion("body.name");
                a.equals = Some(serde_json::json!("Jon"));
                a
            },
            {
                let mut a = make_assertion("body.items");
                a.is_array = Some(true);
                a
            },
        ];
        let results = evaluate_assertions(&assertions, &resp);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn assertion_yaml_round_trip() {
        let yaml = r#"
- field: status
  equals: 200
- field: body.name
  equals: "Jon"
- field: body.items
  is_array: true
- field: elapsed_ms
  less_than: 500.0
- field: header.content-type
  contains: "application/json"
"#;
        let assertions: Vec<Assertion> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(assertions.len(), 5);
        assert_eq!(assertions[0].field, "status");
        assert_eq!(assertions[0].equals, Some(serde_json::json!(200)));
        assert_eq!(assertions[3].less_than, Some(500.0));
        assert_eq!(assertions[4].contains, Some("application/json".to_string()));

        // Serialize back
        let serialized = serde_yaml::to_string(&assertions).unwrap();
        let reparsed: Vec<Assertion> = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.len(), 5);
    }
}
