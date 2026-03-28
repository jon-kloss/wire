pub mod collection;
pub mod error;
pub mod history;
pub mod http;
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
}
