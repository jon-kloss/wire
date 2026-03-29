mod extract;

use crate::collection::load_request;
use crate::http::{execute, HttpClient};
use crate::variables::VariableScope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

pub use extract::extract_from_response;

/// A single step in a request chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainStep {
    /// Path to the request file (relative to .wire/requests/)
    pub run: String,
    /// Variables to extract from the response: { var_name: "body.field" | "headers.name" | "status" }
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extract: HashMap<String, String>,
    /// If true, persist extracted variables to the active environment file
    #[serde(default, skip_serializing_if = "is_false")]
    pub persist: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// Result of executing a single chain step.
#[derive(Debug, Clone, Serialize)]
pub struct ChainStepResult {
    pub step_index: usize,
    pub request_name: String,
    pub request_path: String,
    pub status: u16,
    pub elapsed_ms: u64,
    pub extracted: HashMap<String, String>,
    pub passed: bool,
    pub error: Option<String>,
}

/// Result of executing an entire chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainResult {
    pub steps: Vec<ChainStepResult>,
    pub success: bool,
    pub total_elapsed_ms: u64,
    pub error: Option<String>,
}

/// Execute a chain of request steps sequentially.
///
/// For each step:
/// 1. Load the referenced request file
/// 2. Resolve templates and inject chain variables into scope
/// 3. Execute the request
/// 4. Extract configured response values
/// 5. Add extracted values to scope for subsequent steps
///
/// Stops on first failure (non-2xx or extraction error).
pub async fn execute_chain(
    steps: &[ChainStep],
    wire_dir: &Path,
    scope: &VariableScope,
    client: &HttpClient,
) -> ChainResult {
    let chain_start = Instant::now();
    let mut step_results = Vec::new();
    let requests_dir = wire_dir.join("requests");
    // Clone scope so chain variables don't leak into the caller
    let mut scope = scope.clone();

    for (idx, step) in steps.iter().enumerate() {
        // Resolve request file path
        let request_path = resolve_request_path(&step.run, &requests_dir);

        // Load the request
        let request = match load_request(&request_path) {
            Ok(r) => r,
            Err(e) => {
                let result = ChainStepResult {
                    step_index: idx,
                    request_name: step.run.clone(),
                    request_path: request_path.to_string_lossy().to_string(),
                    status: 0,
                    elapsed_ms: 0,
                    extracted: HashMap::new(),
                    passed: false,
                    error: Some(format!("Failed to load request '{}': {e}", step.run)),
                };
                step_results.push(result);
                return ChainResult {
                    success: false,
                    total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
                    error: Some(format!("Step {} ('{}') failed to load", idx + 1, step.run)),
                    steps: step_results,
                };
            }
        };

        // Execute the request
        let response = match execute(client, &request, &scope).await {
            Ok(r) => r,
            Err(e) => {
                let result = ChainStepResult {
                    step_index: idx,
                    request_name: request.name.clone(),
                    request_path: request_path.to_string_lossy().to_string(),
                    status: 0,
                    elapsed_ms: 0,
                    extracted: HashMap::new(),
                    passed: false,
                    error: Some(format!("Request failed: {e}")),
                };
                step_results.push(result);
                return ChainResult {
                    success: false,
                    total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
                    error: Some(format!("Step {} ('{}') failed: {e}", idx + 1, request.name)),
                    steps: step_results,
                };
            }
        };

        // Check for non-2xx status (3xx, 4xx, 5xx all halt the chain)
        if response.status >= 300 {
            let result = ChainStepResult {
                step_index: idx,
                request_name: request.name.clone(),
                request_path: request_path.to_string_lossy().to_string(),
                status: response.status,
                elapsed_ms: response.elapsed.as_millis() as u64,
                extracted: HashMap::new(),
                passed: false,
                error: Some(format!("HTTP {} {}", response.status, response.status_text)),
            };
            step_results.push(result);
            return ChainResult {
                success: false,
                total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
                error: Some(format!(
                    "Step {} ('{}') returned HTTP {}",
                    idx + 1,
                    request.name,
                    response.status
                )),
                steps: step_results,
            };
        }

        // Extract variables from response
        let extracted = if !step.extract.is_empty() {
            match extract_from_response(&response, &step.extract) {
                Ok(vars) => vars,
                Err(e) => {
                    let result = ChainStepResult {
                        step_index: idx,
                        request_name: request.name.clone(),
                        request_path: request_path.to_string_lossy().to_string(),
                        status: response.status,
                        elapsed_ms: response.elapsed.as_millis() as u64,
                        extracted: HashMap::new(),
                        passed: false,
                        error: Some(format!("Extraction failed: {e}")),
                    };
                    step_results.push(result);
                    return ChainResult {
                        success: false,
                        total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
                        error: Some(format!(
                            "Step {} ('{}') extraction failed: {e}",
                            idx + 1,
                            request.name
                        )),
                        steps: step_results,
                    };
                }
            }
        } else {
            HashMap::new()
        };

        // Push extracted variables into scope for subsequent steps
        if !extracted.is_empty() {
            scope.push_layer(extracted.clone());
        }

        let result = ChainStepResult {
            step_index: idx,
            request_name: request.name.clone(),
            request_path: request_path.to_string_lossy().to_string(),
            status: response.status,
            elapsed_ms: response.elapsed.as_millis() as u64,
            extracted,
            passed: true,
            error: None,
        };
        step_results.push(result);
    }

    ChainResult {
        success: true,
        total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
        error: None,
        steps: step_results,
    }
}

/// Resolve a request path from a step's `run` field.
/// Supports:
/// - Bare name: "login" -> requests/login.wire.yaml
/// - With folder: "auth/login" -> requests/auth/login.wire.yaml
/// - Already has extension: "auth/login.wire.yaml" -> requests/auth/login.wire.yaml
fn resolve_request_path(run: &str, requests_dir: &Path) -> std::path::PathBuf {
    let path = if run.ends_with(".wire.yaml") {
        requests_dir.join(run)
    } else {
        requests_dir.join(format!("{run}.wire.yaml"))
    };
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolve_request_path_bare_name() {
        let dir = PathBuf::from("/project/.wire/requests");
        assert_eq!(
            resolve_request_path("login", &dir),
            PathBuf::from("/project/.wire/requests/login.wire.yaml")
        );
    }

    #[test]
    fn resolve_request_path_with_folder() {
        let dir = PathBuf::from("/project/.wire/requests");
        assert_eq!(
            resolve_request_path("auth/login", &dir),
            PathBuf::from("/project/.wire/requests/auth/login.wire.yaml")
        );
    }

    #[test]
    fn resolve_request_path_with_extension() {
        let dir = PathBuf::from("/project/.wire/requests");
        assert_eq!(
            resolve_request_path("auth/login.wire.yaml", &dir),
            PathBuf::from("/project/.wire/requests/auth/login.wire.yaml")
        );
    }

    #[test]
    fn chain_step_yaml_round_trip() {
        let yaml = r#"
run: auth/login
extract:
  token: body.token
  session_id: headers.x-session-id
persist: true
"#;
        let step: ChainStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.run, "auth/login");
        assert_eq!(step.extract.get("token").unwrap(), "body.token");
        assert_eq!(
            step.extract.get("session_id").unwrap(),
            "headers.x-session-id"
        );
        assert!(step.persist);
    }

    #[test]
    fn chain_step_minimal_yaml() {
        let yaml = "run: users/list\n";
        let step: ChainStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.run, "users/list");
        assert!(step.extract.is_empty());
        assert!(!step.persist);
    }

    #[test]
    fn chain_step_skip_empty_fields_on_serialize() {
        let step = ChainStep {
            run: "test".to_string(),
            extract: HashMap::new(),
            persist: false,
        };
        let yaml = serde_yaml::to_string(&step).unwrap();
        // Use colon suffix to avoid false matches from field values
        assert!(!yaml.contains("extract:"));
        assert!(!yaml.contains("persist:"));
        assert!(yaml.contains("run:"));
    }

    #[test]
    fn chain_step_with_chain_field_in_wire_request_yaml() {
        let yaml = r#"
name: Auth Flow
method: GET
url: https://example.com
chain:
  - run: auth/login
    extract:
      token: body.token
  - run: users/profile
"#;
        let request: crate::collection::WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.chain.len(), 2);
        assert_eq!(request.chain[0].run, "auth/login");
        assert_eq!(request.chain[0].extract.get("token").unwrap(), "body.token");
        assert_eq!(request.chain[1].run, "users/profile");
        assert!(request.chain[1].extract.is_empty());
    }

    #[test]
    fn wire_request_without_chain_has_empty_vec() {
        let yaml = "name: Simple\nmethod: GET\nurl: https://example.com\n";
        let request: crate::collection::WireRequest = serde_yaml::from_str(yaml).unwrap();
        assert!(request.chain.is_empty());
    }
}
