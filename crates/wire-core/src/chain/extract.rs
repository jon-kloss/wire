use crate::error::WireError;
use crate::http::WireResponse;
use crate::test::dotpath;
use std::collections::HashMap;

/// Extract variables from a response based on dot-path extraction rules.
///
/// Extraction paths support three prefixes:
/// - `body.<path>` — extract from JSON response body using dotpath
/// - `headers.<name>` — extract a response header value
/// - `status` — extract the HTTP status code
///
/// Returns a map of variable names to their string values.
pub fn extract_from_response(
    response: &WireResponse,
    extractions: &HashMap<String, String>,
) -> Result<HashMap<String, String>, WireError> {
    let mut result = HashMap::new();

    for (var_name, path) in extractions {
        let value = extract_single(response, path).ok_or_else(|| {
            WireError::Other(format!(
                "Failed to extract '{}' from path '{}'",
                var_name, path
            ))
        })?;
        result.insert(var_name.clone(), value);
    }

    Ok(result)
}

/// Extract a single value from a response using a dot-path.
fn extract_single(response: &WireResponse, path: &str) -> Option<String> {
    if path == "status" {
        return Some(response.status.to_string());
    }

    if let Some(header_name) = path.strip_prefix("headers.") {
        // Case-insensitive header lookup
        let lower = header_name.to_lowercase();
        return response
            .headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.clone());
    }

    if let Some(body_path) = path.strip_prefix("body.") {
        let json: serde_json::Value = serde_json::from_str(&response.body).ok()?;
        let value = dotpath::extract(&json, body_path)?;
        return Some(json_value_to_string(&value));
    }

    // If no prefix, try as body path for convenience
    let json: serde_json::Value = serde_json::from_str(&response.body).ok()?;
    let value = dotpath::extract(&json, path)?;
    Some(json_value_to_string(&value))
}

/// Convert a JSON value to a string suitable for variable interpolation.
fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn mock_response(status: u16, body: &str, headers: Vec<(&str, &str)>) -> WireResponse {
        let mut header_map = HashMap::new();
        for (k, v) in headers {
            header_map.insert(k.to_string(), v.to_string());
        }
        WireResponse {
            status,
            status_text: "OK".to_string(),
            headers: header_map,
            body: body.to_string(),
            elapsed: Duration::from_millis(50),
            size_bytes: body.len(),
        }
    }

    #[test]
    fn extract_status() {
        let response = mock_response(201, "{}", vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("code".to_string(), "status".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["code"], "201");
    }

    #[test]
    fn extract_header() {
        let response = mock_response(200, "{}", vec![("x-request-id", "abc-123")]);
        let mut extractions = HashMap::new();
        extractions.insert("req_id".to_string(), "headers.x-request-id".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["req_id"], "abc-123");
    }

    #[test]
    fn extract_header_case_insensitive() {
        let response = mock_response(200, "{}", vec![("X-Token", "secret")]);
        let mut extractions = HashMap::new();
        extractions.insert("token".to_string(), "headers.x-token".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["token"], "secret");
    }

    #[test]
    fn extract_body_field() {
        let response = mock_response(200, r#"{"token": "jwt-abc", "user": {"id": 42}}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("token".to_string(), "body.token".to_string());
        extractions.insert("user_id".to_string(), "body.user.id".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["token"], "jwt-abc");
        assert_eq!(result["user_id"], "42");
    }

    #[test]
    fn extract_body_array_field() {
        let response = mock_response(200, r#"{"items": [{"id": 1}, {"id": 2}]}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("first_id".to_string(), "body.items[0].id".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["first_id"], "1");
    }

    #[test]
    fn extract_body_string_value_unquoted() {
        let response = mock_response(200, r#"{"name": "Wire"}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("name".to_string(), "body.name".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        // String values should not be JSON-quoted
        assert_eq!(result["name"], "Wire");
    }

    #[test]
    fn extract_missing_field_fails() {
        let response = mock_response(200, r#"{"name": "Wire"}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("missing".to_string(), "body.nonexistent".to_string());

        let result = extract_from_response(&response, &extractions);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn extract_missing_header_fails() {
        let response = mock_response(200, "{}", vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("token".to_string(), "headers.x-missing".to_string());

        let result = extract_from_response(&response, &extractions);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("x-missing"));
    }

    #[test]
    fn extract_from_non_json_body_fails() {
        let response = mock_response(200, "not json", vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("field".to_string(), "body.something".to_string());

        let result = extract_from_response(&response, &extractions);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("something"));
    }

    #[test]
    fn extract_from_empty_body_string_fails() {
        let response = mock_response(200, "", vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("field".to_string(), "body.token".to_string());

        let result = extract_from_response(&response, &extractions);
        assert!(result.is_err());
    }

    #[test]
    fn extract_empty_extractions_returns_empty() {
        let response = mock_response(200, "{}", vec![]);
        let extractions = HashMap::new();

        let result = extract_from_response(&response, &extractions).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn extract_multiple_sources() {
        let response = mock_response(
            200,
            r#"{"data": {"id": 99}}"#,
            vec![("x-session", "sess-abc")],
        );
        let mut extractions = HashMap::new();
        extractions.insert("status_code".to_string(), "status".to_string());
        extractions.insert("data_id".to_string(), "body.data.id".to_string());
        extractions.insert("session".to_string(), "headers.x-session".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["status_code"], "200");
        assert_eq!(result["data_id"], "99");
        assert_eq!(result["session"], "sess-abc");
    }

    #[test]
    fn extract_null_body_value() {
        let response = mock_response(200, r#"{"field": null}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("val".to_string(), "body.field".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["val"], "");
    }

    #[test]
    fn extract_boolean_body_value() {
        let response = mock_response(200, r#"{"active": true}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("active".to_string(), "body.active".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["active"], "true");
    }

    #[test]
    fn extract_without_prefix_falls_back_to_body() {
        let response = mock_response(200, r#"{"token": "abc"}"#, vec![]);
        let mut extractions = HashMap::new();
        extractions.insert("token".to_string(), "token".to_string());

        let result = extract_from_response(&response, &extractions).unwrap();
        assert_eq!(result["token"], "abc");
    }
}
