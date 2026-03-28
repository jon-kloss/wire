use crate::error::WireError;
use crate::variables::VariableScope;
use regex::Regex;
use std::sync::LazyLock;

static VAR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{(\s*[\w.-]+\s*)\}\}").unwrap());

/// Replace all {{variable_name}} occurrences in `template` with values from `scope`.
/// Returns WireError::VariableNotFound if any variable cannot be resolved.
pub fn interpolate(template: &str, scope: &VariableScope) -> Result<String, WireError> {
    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for cap in VAR_PATTERN.captures_iter(template) {
        let full_match = cap.get(0).unwrap();
        let var_name = cap[1].trim();

        result.push_str(&template[last_end..full_match.start()]);

        match scope.resolve(var_name) {
            Some(value) => result.push_str(value),
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
    map.iter()
        .map(|(k, v)| Ok((k.clone(), interpolate(v, scope)?)))
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
