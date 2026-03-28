use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An environment file from .wire/envs/*.yaml
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}
