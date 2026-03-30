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
}

/// Result of executing a single chain step.
#[derive(Debug, Clone, Serialize)]
pub struct ChainStepResult {
    pub step_index: usize,
    pub request_name: String,
    pub request_path: String,
    pub status: u16,
    pub status_text: String,
    pub elapsed_ms: u64,
    pub extracted: HashMap<String, String>,
    pub passed: bool,
    pub error: Option<String>,
    /// The request that was sent (method, url, headers)
    pub request_method: String,
    pub request_url: String,
    pub request_headers: HashMap<String, String>,
    /// The response received
    pub response_headers: HashMap<String, String>,
    pub response_body: String,
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
                let result = empty_step_result(idx, &step.run, &request_path);
                let result = ChainStepResult {
                    error: Some(format!("Failed to load request '{}': {e}", step.run)),
                    ..result
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
                let result = empty_step_result(idx, &request.name, &request_path);
                let result = ChainStepResult {
                    request_method: request.method.clone(),
                    request_url: request.url.clone(),
                    request_headers: request.headers.clone(),
                    error: Some(format!("Request failed: {e}")),
                    ..result
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

        // Build step result with full request/response data
        let build_result = |extracted: HashMap<String, String>,
                            passed: bool,
                            error: Option<String>|
         -> ChainStepResult {
            ChainStepResult {
                step_index: idx,
                request_name: request.name.clone(),
                request_path: request_path.to_string_lossy().to_string(),
                status: response.status,
                status_text: response.status_text.clone(),
                elapsed_ms: response.elapsed.as_millis() as u64,
                extracted,
                passed,
                error,
                request_method: request.method.clone(),
                request_url: request.url.clone(),
                request_headers: request.headers.clone(),
                response_headers: response.headers.clone(),
                response_body: response.body.clone(),
            }
        };

        // Check for non-2xx status (3xx, 4xx, 5xx all halt the chain)
        if response.status >= 300 {
            let result = build_result(
                HashMap::new(),
                false,
                Some(format!("HTTP {} {}", response.status, response.status_text)),
            );
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
                    let result = build_result(
                        HashMap::new(),
                        false,
                        Some(format!("Extraction failed: {e}")),
                    );
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

        step_results.push(build_result(extracted, true, None));
    }

    ChainResult {
        success: true,
        total_elapsed_ms: chain_start.elapsed().as_millis() as u64,
        error: None,
        steps: step_results,
    }
}

/// Create an empty step result for error cases before a response is available.
fn empty_step_result(idx: usize, name: &str, path: &Path) -> ChainStepResult {
    ChainStepResult {
        step_index: idx,
        request_name: name.to_string(),
        request_path: path.to_string_lossy().to_string(),
        status: 0,
        status_text: String::new(),
        elapsed_ms: 0,
        extracted: HashMap::new(),
        passed: false,
        error: None,
        request_method: String::new(),
        request_url: String::new(),
        request_headers: HashMap::new(),
        response_headers: HashMap::new(),
        response_body: String::new(),
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

    /// Helper: create a .wire/ structure with request files pointing at a given base_url.
    /// Returns the wire_dir path.
    fn setup_wire_dir(
        dir: &std::path::Path,
        requests: &[(&str, &str, &str)], // (subfolder/name, method, url_path)
    ) -> PathBuf {
        let wire_dir = dir.join(".wire");
        let requests_dir = wire_dir.join("requests");
        std::fs::create_dir_all(&requests_dir).unwrap();

        // Write wire.yaml
        std::fs::write(wire_dir.join("wire.yaml"), "name: Test\nversion: 1\n").unwrap();

        for (name, method, url_path) in requests {
            let file_path = requests_dir.join(format!("{name}.wire.yaml"));
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let yaml =
                format!("name: {name}\nmethod: {method}\nurl: \"{{{{base_url}}}}{url_path}\"\n");
            std::fs::write(&file_path, yaml).unwrap();
        }

        wire_dir
    }

    fn scope_with_base_url(base_url: &str) -> VariableScope {
        let mut scope = VariableScope::new();
        let mut vars = HashMap::new();
        vars.insert("base_url".to_string(), base_url.to_string());
        scope.push_layer(vars);
        scope
    }

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
"#;
        let step: ChainStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.run, "auth/login");
        assert_eq!(step.extract.get("token").unwrap(), "body.token");
        assert_eq!(
            step.extract.get("session_id").unwrap(),
            "headers.x-session-id"
        );
    }

    #[test]
    fn chain_step_minimal_yaml() {
        let yaml = "run: users/list\n";
        let step: ChainStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.run, "users/list");
        assert!(step.extract.is_empty());
    }

    #[test]
    fn chain_step_skip_empty_fields_on_serialize() {
        let step = ChainStep {
            run: "test".to_string(),
            extract: HashMap::new(),
        };
        let yaml = serde_yaml::to_string(&step).unwrap();
        assert!(!yaml.contains("extract:"));
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

    // --- Async execute_chain tests using wiremock ---

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn execute_chain_empty_steps_succeeds() {
        let client = HttpClient::new().unwrap();
        let scope = VariableScope::new();
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(dir.path(), &[]);

        let result = execute_chain(&[], &wire_dir, &scope, &client).await;
        assert!(result.success);
        assert!(result.steps.is_empty());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn execute_chain_single_step_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/uuid"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"uuid": "abc-123"})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(dir.path(), &[("get-uuid", "GET", "/api/uuid")]);
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        let steps = vec![ChainStep {
            run: "get-uuid".to_string(),
            extract: {
                let mut m = HashMap::new();
                m.insert("id".to_string(), "body.uuid".to_string());
                m
            },
        }];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(result.success);
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].status, 200);
        assert_eq!(result.steps[0].extracted.get("id").unwrap(), "abc-123");
    }

    #[tokio::test]
    async fn execute_chain_variable_forwarding_between_steps() {
        let server = MockServer::start().await;

        // Step 1: returns a token
        Mock::given(method("GET"))
            .and(path("/api/login"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "jwt-xyz"})),
            )
            .mount(&server)
            .await;

        // Step 2: returns profile (the token would be in scope for interpolation)
        Mock::given(method("GET"))
            .and(path("/api/profile"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"name": "Alice", "role": "admin"})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(
            dir.path(),
            &[
                ("auth/login", "GET", "/api/login"),
                ("users/profile", "GET", "/api/profile"),
            ],
        );
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        let steps = vec![
            ChainStep {
                run: "auth/login".to_string(),
                extract: {
                    let mut m = HashMap::new();
                    m.insert("token".to_string(), "body.token".to_string());
                    m
                },
            },
            ChainStep {
                run: "users/profile".to_string(),
                extract: {
                    let mut m = HashMap::new();
                    m.insert("user_name".to_string(), "body.name".to_string());
                    m
                },
            },
        ];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(result.success);
        assert_eq!(result.steps.len(), 2);
        // Step 1 extracted token
        assert_eq!(result.steps[0].extracted.get("token").unwrap(), "jwt-xyz");
        // Step 2 extracted name
        assert_eq!(result.steps[1].extracted.get("user_name").unwrap(), "Alice");
    }

    #[tokio::test]
    async fn execute_chain_halts_on_non_2xx() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/fail"))
            .respond_with(
                ResponseTemplate::new(404).set_body_json(serde_json::json!({"error": "not found"})),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/never"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0) // should never be called
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(
            dir.path(),
            &[
                ("step-ok", "GET", "/api/ok"),
                ("step-fail", "GET", "/api/fail"),
                ("step-never", "GET", "/api/never"),
            ],
        );
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        let steps = vec![
            ChainStep {
                run: "step-ok".to_string(),
                extract: HashMap::new(),
            },
            ChainStep {
                run: "step-fail".to_string(),
                extract: HashMap::new(),
            },
            ChainStep {
                run: "step-never".to_string(),
                extract: HashMap::new(),
            },
        ];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(!result.success);
        assert_eq!(result.steps.len(), 2); // step 3 never ran
        assert!(result.steps[0].passed);
        assert!(!result.steps[1].passed);
        assert!(result.error.unwrap().contains("HTTP 404"));
    }

    #[tokio::test]
    async fn execute_chain_boundary_299_passes_300_fails() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/299"))
            .respond_with(ResponseTemplate::new(299))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/300"))
            .respond_with(ResponseTemplate::new(300))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(
            dir.path(),
            &[
                ("step-299", "GET", "/api/299"),
                ("step-300", "GET", "/api/300"),
            ],
        );
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        // 299 should pass
        let steps_299 = vec![ChainStep {
            run: "step-299".to_string(),
            extract: HashMap::new(),
        }];
        let result = execute_chain(&steps_299, &wire_dir, &scope, &client).await;
        assert!(result.success);

        // 300 should fail
        let steps_300 = vec![ChainStep {
            run: "step-300".to_string(),
            extract: HashMap::new(),
        }];
        let result = execute_chain(&steps_300, &wire_dir, &scope, &client).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("HTTP 300"));
    }

    #[tokio::test]
    async fn execute_chain_extraction_failure_halts() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/data"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"name": "Alice"})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(dir.path(), &[("data", "GET", "/api/data")]);
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        let steps = vec![ChainStep {
            run: "data".to_string(),
            extract: {
                let mut m = HashMap::new();
                m.insert("missing".to_string(), "body.nonexistent.field".to_string());
                m
            },
        }];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("extraction failed"));
    }

    #[tokio::test]
    async fn execute_chain_load_failure_halts() {
        let client = HttpClient::new().unwrap();
        let scope = VariableScope::new();
        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(dir.path(), &[]);

        let steps = vec![ChainStep {
            run: "nonexistent".to_string(),
            extract: HashMap::new(),
        }];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("failed to load"));
    }

    #[tokio::test]
    async fn execute_chain_scope_isolation() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/data"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"secret": "leaked"})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wire_dir = setup_wire_dir(dir.path(), &[("data", "GET", "/api/data")]);
        let scope = scope_with_base_url(&server.uri());
        let client = HttpClient::new().unwrap();

        let steps = vec![ChainStep {
            run: "data".to_string(),
            extract: {
                let mut m = HashMap::new();
                m.insert("secret".to_string(), "body.secret".to_string());
                m
            },
        }];

        let result = execute_chain(&steps, &wire_dir, &scope, &client).await;
        assert!(result.success);

        // Caller's scope should NOT have the extracted variable
        assert_eq!(scope.resolve("secret"), None);
    }
}
