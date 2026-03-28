use std::collections::HashMap;

/// Variable scope for {{variable}} interpolation.
/// Scoping order: Global → Environment → Collection → Request.
/// Later scopes override earlier ones.
#[derive(Debug, Clone, Default)]
pub struct VariableScope {
    layers: Vec<HashMap<String, String>>,
}

impl VariableScope {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Push a new layer of variables (higher priority).
    pub fn push_layer(&mut self, vars: HashMap<String, String>) {
        self.layers.push(vars);
    }

    /// Resolve a variable name by searching layers from top (highest priority) to bottom.
    pub fn resolve(&self, name: &str) -> Option<&str> {
        for layer in self.layers.iter().rev() {
            if let Some(value) = layer.get(name) {
                return Some(value.as_str());
            }
        }
        None
    }

    /// Get all resolved variables (later layers override earlier ones).
    pub fn resolved_map(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for layer in &self.layers {
            result.extend(layer.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        result
    }
}
