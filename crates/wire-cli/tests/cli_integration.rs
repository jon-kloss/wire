use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn wire_cmd() -> Command {
    Command::cargo_bin("wire").unwrap()
}

fn create_sample_collection(dir: &std::path::Path) {
    let wire_dir = dir.join(".wire");
    fs::create_dir_all(wire_dir.join("envs")).unwrap();
    fs::create_dir_all(wire_dir.join("requests/auth")).unwrap();

    fs::write(
        wire_dir.join("wire.yaml"),
        "name: Test Collection\nversion: 1\nactive_env: dev\n",
    )
    .unwrap();

    fs::write(
        wire_dir.join("envs/dev.yaml"),
        "name: Development\nvariables:\n  base_url: https://httpbin.org\n",
    )
    .unwrap();

    fs::write(
        wire_dir.join("requests/auth/login.wire.yaml"),
        "name: Login\nmethod: GET\nurl: \"{{base_url}}/get\"\n",
    )
    .unwrap();
}

#[test]
fn list_collection() {
    let dir = TempDir::new().unwrap();
    create_sample_collection(dir.path());

    wire_cmd()
        .arg("list")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Collection"))
        .stdout(predicate::str::contains("Login"))
        .stdout(predicate::str::contains("dev"));
}

#[test]
fn list_nonexistent_dir_fails() {
    wire_cmd()
        .arg("list")
        .arg("/nonexistent/path/.wire")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Directory not found"));
}

#[test]
fn send_nonexistent_file_fails() {
    wire_cmd()
        .arg("send")
        .arg("/nonexistent/request.wire.yaml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn send_request_with_collection() {
    let dir = TempDir::new().unwrap();
    create_sample_collection(dir.path());

    let request_path = dir.path().join(".wire/requests/auth/login.wire.yaml");

    wire_cmd()
        .arg("send")
        .arg(request_path.to_str().unwrap())
        .arg("-d")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("200"));
}

#[test]
fn history_empty_on_fresh_dir() {
    let dir = TempDir::new().unwrap();

    wire_cmd()
        .arg("history")
        .arg("-d")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("No history entries"));
}

#[test]
fn history_clear_idempotent() {
    let dir = TempDir::new().unwrap();

    // Clear on empty should succeed
    // -d flag must come before the subcommand
    wire_cmd()
        .arg("history")
        .arg("-d")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .arg("clear")
        .assert()
        .success()
        .stdout(predicate::str::contains("cleared"));
}

#[test]
fn send_then_history_shows_entry() {
    let dir = TempDir::new().unwrap();
    create_sample_collection(dir.path());

    let request_path = dir.path().join(".wire/requests/auth/login.wire.yaml");
    let wire_dir = dir.path().join(".wire");

    // Send a request
    wire_cmd()
        .arg("send")
        .arg(request_path.to_str().unwrap())
        .arg("-d")
        .arg(wire_dir.to_str().unwrap())
        .assert()
        .success();

    // History should now show the entry
    // Note: history stores the raw template URL, not the interpolated one
    wire_cmd()
        .arg("history")
        .arg("-d")
        .arg(wire_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("base_url"))
        .stdout(predicate::str::contains("GET"))
        .stdout(predicate::str::contains("200"));
}

#[test]
fn help_flag_works() {
    wire_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Wire"));
}

#[test]
fn version_flag_works() {
    wire_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("wire"));
}

fn create_express_project(dir: &std::path::Path) {
    let routes_dir = dir.join("routes");
    fs::create_dir_all(&routes_dir).unwrap();

    // package.json needed for Express framework detection
    fs::write(
        dir.join("package.json"),
        r#"{"name": "test-api", "dependencies": {"express": "^4.18.0"}}"#,
    )
    .unwrap();

    fs::write(
        routes_dir.join("users.js"),
        r#"const express = require('express');
const router = express.Router();

router.get('/users', (req, res) => {
  res.json([]);
});

router.post('/users', (req, res) => {
  res.json({ id: 1 });
});

module.exports = router;
"#,
    )
    .unwrap();
}

#[test]
fn generate_creates_collection_from_express_project() {
    let dir = TempDir::new().unwrap();
    create_express_project(dir.path());

    wire_cmd()
        .arg("generate")
        .arg(dir.path().to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("2 endpoints discovered"))
        .stdout(predicate::str::contains("Framework: Express"))
        .stdout(predicate::str::contains("Collection"))
        .stdout(predicate::str::contains("created"))
        .stdout(predicate::str::contains("users/"));

    // Verify .wire directory structure
    assert!(dir.path().join(".wire/wire.yaml").exists());
    assert!(dir.path().join(".wire/requests").is_dir());
    assert!(dir.path().join(".wire/envs/dev.yaml").exists());

    // Verify wire.yaml is valid and has collection name
    let metadata = fs::read_to_string(dir.path().join(".wire/wire.yaml")).unwrap();
    assert!(metadata.contains("name:"));
    assert!(metadata.contains("version: 1"));
}

#[test]
fn generate_with_output_flag() {
    let project_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();
    create_express_project(project_dir.path());

    wire_cmd()
        .arg("generate")
        .arg(project_dir.path().to_str().unwrap())
        .arg("-o")
        .arg(output_dir.path().to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Collection"))
        .stdout(predicate::str::contains("created"));

    // Verify .wire directory was created in output dir, not project dir
    assert!(output_dir.path().join(".wire/wire.yaml").exists());
    assert!(output_dir.path().join(".wire/requests").is_dir());
    assert!(!project_dir.path().join(".wire").exists());
}

#[test]
fn generate_empty_project_no_collection() {
    let dir = TempDir::new().unwrap();
    // Empty directory — no source files

    wire_cmd()
        .arg("generate")
        .arg(dir.path().to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("0 endpoints discovered"))
        .stdout(predicate::str::contains(
            "No endpoints found. No collection created.",
        ))
        .stdout(predicate::str::contains("Collection").not());

    // No .wire directory should be created
    assert!(!dir.path().join(".wire").exists());
}

#[test]
fn generate_nonexistent_dir_fails() {
    wire_cmd()
        .arg("generate")
        .arg("/nonexistent/project/path")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not a directory"));
}

// --- Chain tests ---

fn create_chain_collection(dir: &std::path::Path) {
    let wire_dir = dir.join(".wire");
    fs::create_dir_all(wire_dir.join("requests")).unwrap();

    fs::write(wire_dir.join("wire.yaml"), "name: Chain Test\nversion: 1\n").unwrap();

    // A simple request that hits httpbin
    fs::write(
        wire_dir.join("requests/get-uuid.wire.yaml"),
        "name: Get UUID\nmethod: GET\nurl: https://httpbin.org/uuid\n",
    )
    .unwrap();

    // A chain request that runs two steps
    fs::write(
        wire_dir.join("requests/my-chain.wire.yaml"),
        r#"name: My Chain
method: GET
url: https://httpbin.org/get
chain:
  - run: get-uuid
    extract:
      uuid: body.uuid
  - run: get-uuid
"#,
    )
    .unwrap();
}

#[test]
fn chain_run_executes_steps() {
    let dir = TempDir::new().unwrap();
    create_chain_collection(dir.path());

    wire_cmd()
        .arg("chain")
        .arg("run")
        .arg(
            dir.path()
                .join(".wire/requests/my-chain.wire.yaml")
                .to_str()
                .unwrap(),
        )
        .arg("-d")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Running chain"))
        .stdout(predicate::str::contains("2 steps"))
        .stdout(predicate::str::contains("Step 1"))
        .stdout(predicate::str::contains("Step 2"))
        .stdout(predicate::str::contains("uuid"))
        .stdout(predicate::str::contains("completed"));
}

#[test]
fn chain_run_no_chain_section_fails() {
    let dir = TempDir::new().unwrap();
    create_chain_collection(dir.path());

    // Run a request that has no chain section
    wire_cmd()
        .arg("chain")
        .arg("run")
        .arg(
            dir.path()
                .join(".wire/requests/get-uuid.wire.yaml")
                .to_str()
                .unwrap(),
        )
        .arg("-d")
        .arg(dir.path().join(".wire").to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no chain section"));
}

#[test]
fn chain_run_nonexistent_file_fails() {
    wire_cmd()
        .arg("chain")
        .arg("run")
        .arg("/nonexistent/chain.wire.yaml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

// --- Env check tests ---

#[test]
fn env_check_with_env_secrets() {
    let dir = TempDir::new().unwrap();
    let wire_dir = dir.path().join(".wire");
    fs::create_dir_all(wire_dir.join("envs")).unwrap();
    fs::write(wire_dir.join("wire.yaml"), "name: Test\nversion: 1\n").unwrap();

    // Set an env var that the secret ref points to
    std::env::set_var("WIRE_ENV_CHECK_TEST", "resolved");

    fs::write(
        wire_dir.join("envs/dev.yaml"),
        "name: Dev\nvariables:\n  base_url: https://example.com\n  token: $env:WIRE_ENV_CHECK_TEST\n",
    )
    .unwrap();

    wire_cmd()
        .arg("env")
        .arg("check")
        .arg("-d")
        .arg(wire_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("secret reference"))
        .stdout(predicate::str::contains("resolved successfully"));

    std::env::remove_var("WIRE_ENV_CHECK_TEST");
}

#[test]
fn env_check_missing_secret_fails() {
    let dir = TempDir::new().unwrap();
    let wire_dir = dir.path().join(".wire");
    fs::create_dir_all(wire_dir.join("envs")).unwrap();
    fs::write(wire_dir.join("wire.yaml"), "name: Test\nversion: 1\n").unwrap();

    fs::write(
        wire_dir.join("envs/prod.yaml"),
        "name: Prod\nvariables:\n  secret_key: $env:WIRE_TOTALLY_MISSING_VAR_XYZ\n",
    )
    .unwrap();

    wire_cmd()
        .arg("env")
        .arg("check")
        .arg("-d")
        .arg(wire_dir.to_str().unwrap())
        .assert()
        .failure()
        .stdout(predicate::str::contains("failed to resolve"));
}

#[test]
fn env_check_no_secrets_shows_message() {
    let dir = TempDir::new().unwrap();
    let wire_dir = dir.path().join(".wire");
    fs::create_dir_all(wire_dir.join("envs")).unwrap();
    fs::write(wire_dir.join("wire.yaml"), "name: Test\nversion: 1\n").unwrap();

    fs::write(
        wire_dir.join("envs/dev.yaml"),
        "name: Dev\nvariables:\n  base_url: https://example.com\n",
    )
    .unwrap();

    wire_cmd()
        .arg("env")
        .arg("check")
        .arg("-d")
        .arg(wire_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("No secret references"));
}
