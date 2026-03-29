use crate::collection::{BodyType, WireRequest};
use crate::error::WireError;
use crate::http::{HttpClient, WireResponse};
use crate::variables::{interpolate, interpolate_map, VariableScope};
use std::collections::HashMap;
use std::time::Instant;

/// Execute a WireRequest against the given HttpClient, resolving variables from scope.
pub async fn execute(
    client: &HttpClient,
    request: &WireRequest,
    scope: &VariableScope,
) -> Result<WireResponse, WireError> {
    // Interpolate URL
    let url = interpolate(&request.url, scope)?;

    // Interpolate headers
    let headers = interpolate_map(&request.headers, scope)?;

    // Interpolate query params
    let params = interpolate_map(&request.params, scope)?;

    // Build request
    let method: reqwest::Method = request
        .method
        .to_uppercase()
        .parse()
        .map_err(|e| WireError::Other(format!("Invalid HTTP method: {e}")))?;

    let mut req = client.inner().request(method, &url);

    // Add headers
    for (key, value) in &headers {
        req = req.header(key, value);
    }

    // Add query params
    if !params.is_empty() {
        req = req.query(&params);
    }

    // Add body
    if let Some(body) = &request.body {
        match body.body_type {
            BodyType::Json => {
                // Interpolate JSON body string values
                let body_str = serde_json::to_string(&body.content)
                    .map_err(|e| WireError::Other(format!("Failed to serialize body: {e}")))?;
                let interpolated_body = interpolate(&body_str, scope)?;
                req = req
                    .header("Content-Type", "application/json")
                    .body(interpolated_body);
            }
            BodyType::Text => {
                let text = body.content.as_str().unwrap_or_default();
                let interpolated = interpolate(text, scope)?;
                req = req.header("Content-Type", "text/plain").body(interpolated);
            }
            BodyType::FormData => {
                // For form data, content should be an object of key-value pairs
                if let Some(obj) = body.content.as_object() {
                    let mut form = HashMap::new();
                    for (k, v) in obj {
                        let val = v.as_str().unwrap_or_default();
                        form.insert(k.clone(), interpolate(val, scope)?);
                    }
                    req = req.form(&form);
                }
            }
        }
    }

    // Execute and time
    let start = Instant::now();
    let response = req.send().await?;
    let elapsed = start.elapsed();

    // Extract response data
    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("Unknown")
        .to_string();

    let mut resp_headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            resp_headers.insert(key.to_string(), v.to_string());
        }
    }

    let body = response.text().await?;
    let size_bytes = body.len();

    Ok(WireResponse {
        status,
        status_text,
        headers: resp_headers,
        body,
        elapsed,
        size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::WireRequest;

    fn simple_get_request(url: &str) -> WireRequest {
        WireRequest {
            name: "Test".to_string(),
            method: "GET".to_string(),
            url: url.to_string(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: Vec::new(),
            chain: Vec::new(),
        }
    }

    #[tokio::test]
    async fn execute_simple_get() {
        let client = HttpClient::new().unwrap();
        let request = simple_get_request("https://httpbin.org/get");
        let scope = VariableScope::new();

        let response = execute(&client, &request, &scope).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(!response.body.is_empty());
        assert!(response.elapsed.as_millis() > 0);
        assert!(response.size_bytes > 0);
    }

    #[tokio::test]
    async fn execute_with_variable_interpolation() {
        let client = HttpClient::new().unwrap();
        let request = WireRequest {
            name: "Test".to_string(),
            method: "GET".to_string(),
            url: "{{base_url}}/get".to_string(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: Vec::new(),
            chain: Vec::new(),
        };

        let mut scope = VariableScope::new();
        let mut vars = HashMap::new();
        vars.insert("base_url".into(), "https://httpbin.org".into());
        scope.push_layer(vars);

        let response = execute(&client, &request, &scope).await.unwrap();
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn execute_with_missing_variable_fails() {
        let client = HttpClient::new().unwrap();
        let request = simple_get_request("{{missing_var}}/get");
        let scope = VariableScope::new();

        let result = execute(&client, &request, &scope).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_post_with_json_body() {
        let client = HttpClient::new().unwrap();
        let request = WireRequest {
            name: "Test POST".to_string(),
            method: "POST".to_string(),
            url: "https://httpbin.org/post".to_string(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: Some(crate::collection::Body {
                body_type: BodyType::Json,
                content: serde_json::json!({"name": "Wire", "version": "0.1"}),
            }),
            extends: None,
            tests: Vec::new(),
            response_schema: Vec::new(),
            chain: Vec::new(),
        };
        let scope = VariableScope::new();

        let response = execute(&client, &request, &scope).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.contains("Wire"));
    }

    #[tokio::test]
    async fn execute_with_query_params() {
        let client = HttpClient::new().unwrap();
        let mut params = HashMap::new();
        params.insert("foo".into(), "bar".into());
        params.insert("count".into(), "{{num}}".into());

        let request = WireRequest {
            name: "Test Params".to_string(),
            method: "GET".to_string(),
            url: "https://httpbin.org/get".to_string(),
            headers: HashMap::new(),
            params,
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: Vec::new(),
            chain: Vec::new(),
        };

        let mut scope = VariableScope::new();
        let mut vars = HashMap::new();
        vars.insert("num".into(), "42".into());
        scope.push_layer(vars);

        let response = execute(&client, &request, &scope).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.contains("foo"));
        assert!(response.body.contains("42"));
    }
}
