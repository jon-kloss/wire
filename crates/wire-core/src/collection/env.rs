use crate::error::WireError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// An environment file from .wire/envs/*.yaml
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// Save an environment to its YAML file in .wire/envs/.
pub fn save_environment(
    wire_dir: &Path,
    env_name: &str,
    env: &Environment,
) -> Result<(), WireError> {
    let path = wire_dir.join("envs").join(format!("{env_name}.yaml"));
    let yaml = serde_yaml::to_string(env)?;
    std::fs::write(path, yaml)?;
    Ok(())
}
