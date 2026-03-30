use crate::collection::{load_collection, load_request, load_request_resolved, WireRequest};
use crate::http::{execute, HttpClient};
use crate::test::{evaluate_assertions, TestResult};
use crate::variables::VariableScope;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Result of running tests for a single request file.
#[derive(Debug, Clone, Serialize)]
pub struct RequestTestResult {
    pub file: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub assertions: Vec<TestResult>,
    pub error: Option<String>,
    /// Response body (needed for snapshot diffing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Response headers (needed for snapshot diffing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

impl RequestTestResult {
    pub fn all_passed(&self) -> bool {
        self.error.is_none() && self.assertions.iter().all(|a| a.passed)
    }
}

/// Summary of a full test run.
#[derive(Debug, Clone, Serialize)]
pub struct TestRunSummary {
    pub results: Vec<RequestTestResult>,
    pub total_assertions: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
}

impl TestRunSummary {
    pub fn all_passed(&self) -> bool {
        self.failed == 0 && self.errors == 0
    }
}

/// Run tests from a file or directory.
///
/// If `path` is a .wire.yaml file, runs tests from that file.
/// If `path` is a directory, walks it for all .wire.yaml files with tests.
pub async fn run_tests(
    path: &Path,
    env_name: Option<&str>,
    wire_dir: Option<&Path>,
) -> Result<TestRunSummary, Box<dyn std::error::Error>> {
    let client = HttpClient::new()?;

    // Build variable scope from environment
    let mut scope = VariableScope::new();
    if let Some(wd) = wire_dir {
        if wd.is_dir() {
            let collection = load_collection(wd)?;
            let active_env = env_name
                .map(|s| s.to_string())
                .or(collection.metadata.active_env);
            if let Some(env_key) = &active_env {
                if let Some(environment) = collection.environments.get(env_key) {
                    scope.push_layer(environment.variables.clone());
                }
            }
        }
    }

    // Collect test files
    let files = collect_test_files(path);

    let mut results = Vec::new();

    for file_path in files {
        let request = match if let Some(wd) = wire_dir {
            load_request_resolved(&file_path, wd)
        } else {
            load_request(&file_path)
        } {
            Ok(r) => r,
            Err(e) => {
                results.push(RequestTestResult {
                    file: file_path.to_string_lossy().to_string(),
                    name: String::new(),
                    method: String::new(),
                    url: String::new(),
                    status: None,
                    assertions: Vec::new(),
                    error: Some(format!("Failed to load: {e}")),
                    response_body: None,
                    headers: None,
                });
                continue;
            }
        };

        if request.tests.is_empty() {
            continue; // Skip files without tests
        }

        let result = run_request_tests(&client, &request, &scope, &file_path).await;
        results.push(result);
    }

    let total_assertions: usize = results.iter().map(|r| r.assertions.len()).sum();
    let passed: usize = results
        .iter()
        .flat_map(|r| &r.assertions)
        .filter(|a| a.passed)
        .count();
    let failed = total_assertions - passed;
    let errors = results.iter().filter(|r| r.error.is_some()).count();

    Ok(TestRunSummary {
        results,
        total_assertions,
        passed,
        failed,
        errors,
    })
}

async fn run_request_tests(
    client: &HttpClient,
    request: &WireRequest,
    scope: &VariableScope,
    file_path: &Path,
) -> RequestTestResult {
    let response = match execute(client, request, scope).await {
        Ok(r) => r,
        Err(e) => {
            return RequestTestResult {
                file: file_path.to_string_lossy().to_string(),
                name: request.name.clone(),
                method: request.method.clone(),
                url: request.url.clone(),
                status: None,
                assertions: Vec::new(),
                error: Some(format!("Request failed: {e}")),
                response_body: None,
                headers: None,
            };
        }
    };

    let assertions = evaluate_assertions(&request.tests, &response);

    RequestTestResult {
        file: file_path.to_string_lossy().to_string(),
        name: request.name.clone(),
        method: request.method.clone(),
        url: request.url.clone(),
        status: Some(response.status),
        assertions,
        error: None,
        response_body: Some(response.body.clone()),
        headers: Some(response.headers.clone()),
    }
}

/// Collect .wire.yaml files from a path (file or directory).
fn collect_test_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }

    let mut files = Vec::new();
    collect_wire_files_recursive(path, &mut files, 10);
    files.sort();
    files
}

fn collect_wire_files_recursive(dir: &Path, files: &mut Vec<PathBuf>, depth: u32) {
    if depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !matches!(name.as_ref(), ".git" | "node_modules" | "target") {
                collect_wire_files_recursive(&path, files, depth - 1);
            }
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".wire.yaml"))
        {
            files.push(path);
        }
    }
}
