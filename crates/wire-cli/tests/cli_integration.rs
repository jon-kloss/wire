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
