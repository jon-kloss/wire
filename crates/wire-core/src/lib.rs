pub mod collection;
pub mod error;
pub mod history;
pub mod http;
pub mod scan;
pub mod test;
pub mod variables;

#[cfg(test)]
mod tests {
    use crate::collection::Environment;
    use crate::collection::{BodyType, WireRequest};
    use crate::variables::VariableScope;
    use std::collections::HashMap;

    #[test]
    fn wire_request_yaml_round_trip() {
        let yaml = r#"
name: Create User
method: POST
url: "{{base_url}}/api/users"
headers:
  Content-Type: application/json
  Authorization: "Bearer {{token}}"
body:
  type: json
  content:
    name: Jon
    email: jon@example.com
params:
  include: profile
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.name, "Create User");
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "{{base_url}}/api/users");
        assert_eq!(
            request.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert!(request.body.is_some());
        let body = request.body.unwrap();
        assert_eq!(body.body_type, BodyType::Json);

        // Round-trip: serialize back to YAML and re-parse
        let serialized = serde_yaml::to_string(&WireRequest {
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            headers: request.headers.clone(),
            params: request.params.clone(),
            body: Some(body.clone()),
            tests: Vec::new(),
        })
        .unwrap();
        let reparsed: WireRequest = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.name, "Create User");
        assert_eq!(reparsed.method, "POST");
    }

    #[test]
    fn environment_yaml_parse() {
        let yaml = r#"
name: Development
variables:
  base_url: http://localhost:3000
  token: dev-token-123
"#;
        let env: Environment = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(env.name, "Development");
        assert_eq!(
            env.variables.get("base_url").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(env.variables.get("token").unwrap(), "dev-token-123");
    }

    #[test]
    fn variable_scope_layering() {
        let mut scope = VariableScope::new();

        let mut global = HashMap::new();
        global.insert("base_url".into(), "http://global.example.com".into());
        global.insert("token".into(), "global-token".into());
        scope.push_layer(global);

        let mut env = HashMap::new();
        env.insert("base_url".into(), "http://dev.example.com".into());
        scope.push_layer(env);

        // Env layer overrides global for base_url
        assert_eq!(scope.resolve("base_url"), Some("http://dev.example.com"));
        // Global token still visible (not overridden)
        assert_eq!(scope.resolve("token"), Some("global-token"));
        // Missing variable returns None
        assert_eq!(scope.resolve("missing"), None);
    }

    #[test]
    fn wire_request_minimal_yaml() {
        let yaml = r#"
name: Simple GET
method: GET
url: https://api.example.com/health
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.name, "Simple GET");
        assert_eq!(request.method, "GET");
        assert!(request.headers.is_empty());
        assert!(request.params.is_empty());
        assert!(request.body.is_none());
    }

    #[test]
    fn malformed_yaml_fails_to_parse() {
        let yaml = "this is not: [valid: yaml: {";
        let result = serde_yaml::from_str::<WireRequest>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn yaml_missing_required_fields() {
        // Missing 'name' field
        let yaml = "method: GET\nurl: https://example.com\n";
        let result = serde_yaml::from_str::<WireRequest>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn body_type_text() {
        let yaml = r#"
name: Text Body
method: POST
url: https://example.com
body:
  type: text
  content: "Hello World"
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        let body = request.body.unwrap();
        assert_eq!(body.body_type, BodyType::Text);
        assert_eq!(body.content.as_str().unwrap(), "Hello World");
    }

    #[test]
    fn body_type_formdata() {
        let yaml = r#"
name: Form Body
method: POST
url: https://example.com
body:
  type: formdata
  content:
    username: jon
    password: secret
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        let body = request.body.unwrap();
        assert_eq!(body.body_type, BodyType::FormData);
        assert_eq!(body.content["username"], "jon");
    }

    #[test]
    fn environment_with_empty_variables() {
        let yaml = "name: Empty Env\nvariables: {}\n";
        let env: Environment = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(env.name, "Empty Env");
        assert!(env.variables.is_empty());
    }

    #[test]
    fn variable_scope_empty_returns_none() {
        let scope = VariableScope::new();
        assert_eq!(scope.resolve("anything"), None);
    }

    #[test]
    fn variable_scope_resolved_map_merges_layers() {
        let mut scope = VariableScope::new();

        let mut global = HashMap::new();
        global.insert("a".into(), "1".into());
        global.insert("b".into(), "2".into());
        scope.push_layer(global);

        let mut env = HashMap::new();
        env.insert("b".into(), "overridden".into());
        env.insert("c".into(), "3".into());
        scope.push_layer(env);

        let resolved = scope.resolved_map();
        assert_eq!(resolved["a"], "1");
        assert_eq!(resolved["b"], "overridden");
        assert_eq!(resolved["c"], "3");
    }

    #[test]
    fn request_with_many_headers() {
        let yaml = r#"
name: Many Headers
method: GET
url: https://example.com
headers:
  Accept: application/json
  Authorization: Bearer tok
  X-Custom-One: value1
  X-Custom-Two: value2
  Content-Type: text/plain
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.headers.len(), 5);
        assert_eq!(request.headers.get("X-Custom-One").unwrap(), "value1");
    }

    #[test]
    fn request_with_query_params() {
        let yaml = r#"
name: With Params
method: GET
url: https://example.com/search
params:
  q: rust
  page: "1"
  limit: "20"
"#;
        let request: WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.params.len(), 3);
        assert_eq!(request.params.get("q").unwrap(), "rust");
    }
}
