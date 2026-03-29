use crate::error::WireError;

/// Resolve a secret from AWS Secrets Manager.
///
/// Key format: `secret-name` or `secret-name/json-key`
/// - `secret-name` — returns the entire secret string
/// - `secret-name/json-key` — returns a specific field from a JSON secret
///
/// Shells out to: `aws secretsmanager get-secret-value --secret-id <name> --query SecretString --output text`
pub fn resolve(key: &str) -> Result<String, WireError> {
    let (secret_name, json_key) = if let Some(pos) = key.find('/') {
        (&key[..pos], Some(&key[pos + 1..]))
    } else {
        (key, None)
    };

    let output = std::process::Command::new("aws")
        .args([
            "secretsmanager",
            "get-secret-value",
            "--secret-id",
            secret_name,
            "--query",
            "SecretString",
            "--output",
            "text",
        ])
        .output()
        .map_err(|e| {
            WireError::Other(format!(
                "Secret not found: $aws:{key} — failed to run 'aws' CLI: {e}. Is the AWS CLI installed?"
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WireError::Other(format!(
            "Secret not found: $aws:{key} — AWS CLI error: {stderr}"
        )));
    }

    let secret_string = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // If a JSON key was specified, extract it from the secret
    if let Some(json_key) = json_key {
        let json: serde_json::Value = serde_json::from_str(&secret_string).map_err(|e| {
            WireError::Other(format!(
                "Secret $aws:{key} — secret '{secret_name}' is not valid JSON: {e}"
            ))
        })?;

        json.get(json_key)
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .ok_or_else(|| {
                WireError::Other(format!(
                    "Secret not found: $aws:{key} — key '{json_key}' not found in secret '{secret_name}'"
                ))
            })
    } else {
        Ok(secret_string)
    }
}
