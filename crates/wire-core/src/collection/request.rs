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
}

fn default_version() -> u32 {
    1
}
