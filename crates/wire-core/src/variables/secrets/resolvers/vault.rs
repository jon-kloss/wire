use crate::error::WireError;

/// Resolve a secret from HashiCorp Vault.
///
/// Key format: `secret/path#field`
/// - `secret/data/app#token` — reads field "token" from secret at path "secret/data/app"
/// - `secret/data/app` — returns the entire secret as JSON string
///
/// Shells out to: `vault kv get -field=<field> <path>` or `vault kv get -format=json <path>`
pub fn resolve(key: &str) -> Result<String, WireError> {
    let (path, field) = if let Some(pos) = key.find('#') {
        (&key[..pos], Some(&key[pos + 1..]))
    } else {
        (key, None)
    };

    let output = if let Some(field) = field {
        std::process::Command::new("vault")
            .args(["kv", "get", &format!("-field={field}"), path])
            .output()
    } else {
        std::process::Command::new("vault")
            .args(["kv", "get", "-format=json", path])
            .output()
    };

    let output = output.map_err(|e| {
        WireError::Other(format!(
            "Secret not found: $vault:{key} — failed to run 'vault' CLI: {e}. Is the Vault CLI installed?"
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WireError::Other(format!(
            "Secret not found: $vault:{key} — Vault CLI error: {stderr}"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
