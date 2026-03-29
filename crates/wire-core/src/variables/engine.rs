use crate::error::WireError;
use crate::variables::secrets;
use crate::variables::VariableScope;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static VAR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{(\s*[\w.-]+\s*)\}\}").unwrap());

/// Replace all {{variable_name}} occurrences in `template` with values from `scope`.
/// Returns WireError::VariableNotFound if any variable cannot be resolved.
/// Secret references ($env:, $dotenv:, $aws:, $vault:) are resolved transparently.
pub fn interpolate(template: &str, scope: &VariableScope) -> Result<String, WireError> {
    interpolate_with_context(template, scope, None)
}

/// Like interpolate(), but with an optional project directory for .env file discovery.
pub fn interpolate_with_context(
    template: &str,
    scope: &VariableScope,
    project_dir: Option<&Path>,
) -> Result<String, WireError> {
    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for cap in VAR_PATTERN.captures_iter(template) {
        let full_match = cap.get(0).unwrap();
        let var_name = cap[1].trim();

        result.push_str(&template[last_end..full_match.start()]);

        match scope.resolve(var_name) {
            Some(value) => {
                // Check if the resolved value is a secret reference
                if let Some(secret_ref) = secrets::parse_secret_ref(value) {
                    let resolved = secrets::resolve_secret(&secret_ref, project_dir)?;
                    result.push_str(&resolved);
                } else {
                    result.push_str(value);
                }
            }
            None => return Err(WireError::VariableNotFound(var_name.to_string())),
        }

        last_end = full_match.end();
    }

    result.push_str(&template[last_end..]);
    Ok(result)
}

/// Same as interpolate but for HashMap values — interpolates each value.
pub fn interpolate_map(
    map: &std::collections::HashMap<String, String>,
    scope: &VariableScope,
) -> Result<std::collections::HashMap<String, String>, WireError> {
    interpolate_map_with_context(map, scope, None)
}

/// Like interpolate_map(), but with an optional project directory for .env file discovery.
pub fn interpolate_map_with_context(
    map: &std::collections::HashMap<String, String>,
    scope: &VariableScope,
    project_dir: Option<&Path>,
) -> Result<std::collections::HashMap<String, String>, WireError> {
    map.iter()
        .map(|(k, v)| Ok((k.clone(), interpolate_with_context(v, scope, project_dir)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scope_with(vars: &[(&str, &str)]) -> VariableScope {
        let mut scope = VariableScope::new();
        let layer: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        scope.push_layer(layer);
        scope
    }

    #[test]
    fn simple_variable() {
        let scope = scope_with(&[("name", "Wire")]);
        assert_eq!(interpolate("Hello {{name}}", &scope).unwrap(), "Hello Wire");
    }

    #[test]
    fn multiple_variables() {
        let scope = scope_with(&[("base_url", "http://localhost:3000"), ("version", "v2")]);
        assert_eq!(
            interpolate("{{base_url}}/api/{{version}}/users", &scope).unwrap(),
            "http://localhost:3000/api/v2/users"
        );
    }

    #[test]
    fn missing_variable_returns_error() {
        let scope = VariableScope::new();
        let result = interpolate("{{missing}}", &scope);
        assert!(result.is_err());
        match result.unwrap_err() {
            WireError::VariableNotFound(name) => assert_eq!(name, "missing"),
            other => panic!("expected VariableNotFound, got: {other}"),
        }
    }

    #[test]
    fn no_variables_passes_through() {
        let scope = VariableScope::new();
        assert_eq!(
            interpolate("no variables here", &scope).unwrap(),
            "no variables here"
        );
    }

    #[test]
    fn whitespace_in_braces() {
        let scope = scope_with(&[("token", "abc123")]);
        assert_eq!(
            interpolate("Bearer {{ token }}", &scope).unwrap(),
            "Bearer abc123"
        );
    }

    #[test]
    fn scoping_override() {
        let mut scope = VariableScope::new();
        let mut global = HashMap::new();
        global.insert("url".into(), "http://global.com".into());
        scope.push_layer(global);

        let mut env = HashMap::new();
        env.insert("url".into(), "http://dev.com".into());
        scope.push_layer(env);

        assert_eq!(
            interpolate("{{url}}/api", &scope).unwrap(),
            "http://dev.com/api"
        );
    }

    #[test]
    fn secret_env_resolves_transparently() {
        std::env::set_var("WIRE_SECRET_TEST_TOKEN", "resolved-token");
        let scope = scope_with(&[("api_key", "$env:WIRE_SECRET_TEST_TOKEN")]);
        let result = interpolate("Bearer {{api_key}}", &scope).unwrap();
        assert_eq!(result, "Bearer resolved-token");
        std::env::remove_var("WIRE_SECRET_TEST_TOKEN");
    }

    #[test]
    fn secret_env_missing_fails_with_clear_error() {
        let scope = scope_with(&[("api_key", "$env:WIRE_NONEXISTENT_SECRET_99")]);
        let result = interpolate("{{api_key}}", &scope);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("WIRE_NONEXISTENT_SECRET_99"));
        assert!(err.contains("env"));
    }

    #[test]
    fn non_secret_dollar_value_passes_through() {
        // Values starting with $ but not matching a known prefix are not secrets
        let scope = scope_with(&[("price", "$99.99")]);
        let result = interpolate("Cost: {{price}}", &scope).unwrap();
        assert_eq!(result, "Cost: $99.99");
    }

    #[test]
    fn mixed_secret_and_regular_vars() {
        std::env::set_var("WIRE_MIX_TEST", "secret-val");
        let scope = scope_with(&[
            ("base_url", "https://api.example.com"),
            ("token", "$env:WIRE_MIX_TEST"),
        ]);
        let result = interpolate("{{base_url}}?token={{token}}", &scope).unwrap();
        assert_eq!(result, "https://api.example.com?token=secret-val");
        std::env::remove_var("WIRE_MIX_TEST");
    }

    #[test]
    fn interpolate_map_works() {
        let scope = scope_with(&[("token", "secret")]);
        let mut map = HashMap::new();
        map.insert("Authorization".into(), "Bearer {{token}}".into());
        map.insert("Accept".into(), "application/json".into());

        let result = interpolate_map(&map, &scope).unwrap();
        assert_eq!(result["Authorization"], "Bearer secret");
        assert_eq!(result["Accept"], "application/json");
    }
}
