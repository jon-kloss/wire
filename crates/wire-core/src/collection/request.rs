use crate::chain::ChainStep;
use crate::test::Assertion;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single API request, deserialized from a .wire.yaml file.
/// Uses a flat, explicit schema — all fields at top level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<Body>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<Assertion>,
    /// Expected response fields from codebase scan (field_name, type_hint)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_schema: Vec<(String, String)>,
    /// Chain steps for multi-request flows
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<ChainStep>,
    /// Snapshot configuration (ignore rules for golden file diffing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SnapshotConfig>,
}

/// Per-request snapshot configuration, parsed from the `snapshot` field in .wire.yaml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotConfig {
    /// Paths to ignore when diffing (e.g. "body.timestamp", "body.users[*].id")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Body {
    #[serde(rename = "type")]
    pub body_type: BodyType,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BodyType {
    Json,
    Text,
    FormData,
}

/// Collection metadata from .wire/wire.yaml
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireCollection {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub active_env: Option<String>,
    /// Legacy single default template (read for backward compat, not written)
    #[serde(default, skip_serializing)]
    pub default_template: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_templates: Vec<String>,
    /// Source project directory (set by Generate from Codebase, used by drift detection)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_dir: Option<String>,
}

impl WireCollection {
    /// Get effective default templates, merging legacy single field into the vec.
    pub fn effective_default_templates(&self) -> Vec<String> {
        if !self.default_templates.is_empty() {
            return self.default_templates.clone();
        }
        // Backward compat: old default_template field
        match &self.default_template {
            Some(t) => vec![t.clone()],
            None => Vec::new(),
        }
    }
}

fn default_version() -> u32 {
    1
}
