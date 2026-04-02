use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::Path;
use wire_core::chain;
use wire_core::collection::{list_templates, load_collection, load_request, load_request_resolved};
use wire_core::drift;
use wire_core::history::{self, HistoryEntry};
use wire_core::http::{execute, HttpClient};
use wire_core::test::runner;
use wire_core::variables::VariableScope;

#[derive(Parser)]
#[command(name = "wire")]
#[command(about = "Wire — a fast, local-first API client")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send an API request from a .wire.yaml file
    Send {
        /// Path to the .wire.yaml request file
        file: String,

        /// Environment to use (e.g., dev, prod)
        #[arg(short, long)]
        env: Option<String>,

        /// Path to .wire collection directory (for environments)
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,

        /// Save the response as a golden file snapshot
        #[arg(long)]
        snapshot: bool,
    },
    /// List requests in a .wire collection directory
    List {
        /// Path to .wire directory (defaults to .wire/ in current dir)
        #[arg(default_value = ".wire")]
        dir: String,
    },
    /// Run tests defined in .wire.yaml files
    Test {
        /// Path to a .wire.yaml file or directory to test
        path: String,

        /// Environment to use (e.g., dev, prod)
        #[arg(short, long)]
        env: Option<String>,

        /// Path to .wire collection directory (for environments)
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        output: String,

        /// Diff response against saved snapshot
        #[arg(long)]
        snapshot: bool,
    },
    /// Execute a request chain (multi-step API flow)
    Chain {
        #[command(subcommand)]
        action: ChainAction,
    },
    /// Manage request templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },
    /// Detect drift between code endpoints and collection requests
    Drift {
        /// Path to source project directory to scan
        project_dir: String,

        /// Path to .wire collection directory
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,

        /// Auto-generate stub files for new endpoints
        #[arg(long)]
        fix: bool,

        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        output: String,
    },
    /// Scan a project's source code and generate a .wire collection
    Generate {
        /// Path to source project directory to scan
        project_dir: String,

        /// Output directory for .wire collection (defaults to project dir)
        #[arg(short = 'o', long)]
        output: Option<String>,
    },
    /// Manage environments and validate secrets
    Env {
        #[command(subcommand)]
        action: EnvAction,
    },
    /// Manage response snapshots (golden files)
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },
    /// Install Wire's Claude Code skill to ~/.claude/commands/
    InstallClaudeSkill,
    /// Remove Wire's Claude Code skill from ~/.claude/commands/
    UninstallClaudeSkill,
    /// View or manage request history
    History {
        #[command(subcommand)]
        action: Option<HistoryAction>,

        /// Maximum number of entries to show
        #[arg(short, long, default_value = "50")]
        limit: usize,

        /// Path to .wire collection directory
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,
    },
}

#[derive(Subcommand)]
enum EnvAction {
    /// Validate all secret references in collection environments
    Check {
        /// Path to .wire collection directory
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,
    },
}

#[derive(Subcommand)]
enum ChainAction {
    /// Run a request chain defined in a .wire.yaml file
    Run {
        /// Path to a .wire.yaml file containing a chain section
        file: String,

        /// Environment to use (e.g., dev, prod)
        #[arg(short, long)]
        env: Option<String>,

        /// Path to .wire collection directory
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    /// List all available templates
    List {
        /// Path to .wire directory
        #[arg(default_value = ".wire")]
        dir: String,
    },
}

#[derive(Subcommand)]
enum SnapshotAction {
    /// Update (overwrite) a snapshot with the current response
    Update {
        /// Path to the .wire.yaml request file
        file: String,

        /// Environment to use (e.g., dev, prod)
        #[arg(short, long)]
        env: Option<String>,

        /// Path to .wire collection directory
        #[arg(short = 'd', long, default_value = ".wire")]
        wire_dir: String,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// Clear all request history
    Clear,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Send {
            file,
            env,
            wire_dir,
            snapshot,
        } => {
            if let Err(e) = cmd_send(&file, env.as_deref(), &wire_dir, snapshot).await {
                eprintln!("{}: {e}", "Error".red().bold());
                std::process::exit(1);
            }
        }
        Commands::Test {
            path,
            env,
            wire_dir,
            output,
            snapshot,
        } => {
            let exit_code = cmd_test(&path, env.as_deref(), &wire_dir, &output, snapshot).await;
            std::process::exit(exit_code);
        }
        Commands::List { dir } => {
            if let Err(e) = cmd_list(&dir) {
                eprintln!("{}: {e}", "Error".red().bold());
                std::process::exit(1);
            }
        }
        Commands::Drift {
            project_dir,
            wire_dir,
            fix,
            output,
        } => {
            let exit_code = cmd_drift(&project_dir, &wire_dir, fix, &output);
            std::process::exit(exit_code);
        }
        Commands::Snapshot { action } => match action {
            SnapshotAction::Update {
                file,
                env,
                wire_dir,
            } => {
                if let Err(e) = cmd_snapshot_update(&file, env.as_deref(), &wire_dir).await {
                    eprintln!("{}: {e}", "Error".red().bold());
                    std::process::exit(1);
                }
            }
        },
        Commands::InstallClaudeSkill => {
            cmd_install_claude_skill();
        }
        Commands::UninstallClaudeSkill => {
            cmd_uninstall_claude_skill();
        }
        Commands::Env { action } => match action {
            EnvAction::Check { wire_dir } => {
                let exit_code = cmd_env_check(&wire_dir);
                std::process::exit(exit_code);
            }
        },
        Commands::Generate {
            project_dir,
            output,
        } => {
            let exit_code = cmd_generate(&project_dir, output.as_deref());
            std::process::exit(exit_code);
        }
        Commands::Chain { action } => match action {
            ChainAction::Run {
                file,
                env,
                wire_dir,
            } => {
                let exit_code = cmd_chain_run(&file, env.as_deref(), &wire_dir).await;
                std::process::exit(exit_code);
            }
        },
        Commands::Template { action } => match action {
            TemplateAction::List { dir } => {
                if let Err(e) = cmd_template_list(&dir) {
                    eprintln!("{}: {e}", "Error".red().bold());
                    std::process::exit(1);
                }
            }
        },
        Commands::History {
            action,
            limit,
            wire_dir,
        } => {
            if let Err(e) = cmd_history(action, limit, &wire_dir) {
                eprintln!("{}: {e}", "Error".red().bold());
                std::process::exit(1);
            }
        }
    }
}

async fn cmd_send(
    file: &str,
    env_name: Option<&str>,
    wire_dir: &str,
    save_snapshot: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load the request (with template resolution)
    let request = load_request_resolved(Path::new(file), Path::new(wire_dir))?;

    // Build variable scope
    let mut scope = VariableScope::new();

    // Try to load environment from collection
    let wire_path = Path::new(wire_dir);
    if wire_path.is_dir() {
        let collection = load_collection(wire_path)?;

        // Determine which environment to use
        let active_env = env_name
            .map(|s| s.to_string())
            .or(collection.metadata.active_env);

        if let Some(env_key) = &active_env {
            if let Some(environment) = collection.environments.get(env_key) {
                scope.push_layer(environment.variables.clone());
            } else {
                eprintln!(
                    "{}: environment '{}' not found in {}",
                    "Warning".yellow().bold(),
                    env_key,
                    wire_dir
                );
            }
        }
    }

    // Execute
    let client = HttpClient::new()?;

    println!(
        "{} {} {}",
        "→".blue().bold(),
        request.method.cyan().bold(),
        request.url
    );

    let response = execute(&client, &request, &scope).await?;

    // Print response
    println!();
    print_status(response.status);
    println!(
        "  {} {}ms  {} {} bytes",
        "Time:".dimmed(),
        response.elapsed.as_millis(),
        "Size:".dimmed(),
        response.size_bytes,
    );
    println!();

    // Print headers
    if !response.headers.is_empty() {
        println!("{}", "Headers:".dimmed());
        for (key, value) in &response.headers {
            println!("  {}: {}", key.cyan(), value);
        }
        println!();
    }

    // Print body (pretty-print JSON if possible)
    println!("{}", "Body:".dimmed());
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response.body) {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", response.body);
    }

    // Fire-and-forget history recording
    let wire_path = Path::new(wire_dir);
    let history_path = if wire_path.is_dir() {
        history::resolve_history_path(Some(wire_path))
    } else {
        history::resolve_history_path(None)
    };
    if let Err(e) = history::save_entry(
        &history_path,
        &HistoryEntry {
            timestamp: chrono::Utc::now(),
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            status: response.status,
            elapsed_ms: response.elapsed.as_millis() as u64,
        },
    ) {
        eprintln!("{}: failed to save history: {e}", "Warning".yellow());
    }

    // Save snapshot if requested
    if save_snapshot {
        let snap = wire_core::snapshot::snapshot_from_response(
            response.status,
            &response.headers,
            &response.body,
        );
        let request_relative = file
            .trim_start_matches(wire_dir)
            .trim_start_matches('/')
            .trim_start_matches('\\');
        match wire_core::snapshot::save_snapshot(&snap, Path::new(wire_dir), request_relative) {
            Ok(path) => {
                println!();
                println!(
                    "{} Snapshot saved: {}",
                    "\u{2713}".green().bold(),
                    path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "{}: failed to save snapshot: {e}",
                    "Warning".yellow().bold()
                );
            }
        }
    }

    Ok(())
}

async fn cmd_test(
    path: &str,
    env_name: Option<&str>,
    wire_dir: &str,
    output: &str,
    check_snapshot: bool,
) -> i32 {
    let wire_path = Path::new(wire_dir);
    let wd = if wire_path.is_dir() {
        Some(wire_path)
    } else {
        None
    };

    let summary = match runner::run_tests(Path::new(path), env_name, wd).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", "Error".red().bold());
            return 1;
        }
    };

    if output == "json" {
        match serde_json::to_string_pretty(&summary) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{}: {e}", "Error".red().bold());
                return 1;
            }
        }
    } else {
        print_test_results(&summary);
    }

    let mut exit_code = if summary.all_passed() { 0 } else { 1 };

    // Snapshot diff if requested
    if check_snapshot {
        let snapshot_exit = run_snapshot_diff(wire_dir, &summary);
        if snapshot_exit != 0 {
            exit_code = snapshot_exit;
        }
    }

    exit_code
}

fn run_snapshot_diff(wire_dir: &str, summary: &runner::TestRunSummary) -> i32 {
    use wire_core::diff::{format::format_diff, ignore::parse_ignore_rules, structural_diff};
    use wire_core::snapshot::{load_snapshot, snapshot_from_response};

    let wire_path = Path::new(wire_dir);
    let mut has_diffs = false;

    for result in &summary.results {
        // We need the response data from the test result
        let Some(ref response_body) = result.response_body else {
            continue;
        };

        let request_relative = result
            .file
            .trim_start_matches(wire_dir)
            .trim_start_matches('/')
            .trim_start_matches('\\');

        let saved = match load_snapshot(wire_path, request_relative) {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!(
                    "\n{} No snapshot for {} — run with {} to create one",
                    "!".yellow().bold(),
                    result.name,
                    "wire send --snapshot".cyan()
                );
                continue;
            }
            Err(e) => {
                eprintln!("{}: loading snapshot: {e}", "Error".red().bold());
                continue;
            }
        };

        let status = result.status.unwrap_or(0);
        let headers = result.headers.as_ref().cloned().unwrap_or_default();
        let current = snapshot_from_response(status, &headers, response_body);

        // Load ignore rules from the request file (with template resolution)
        let ignore_rules =
            if let Ok(req) = load_request_resolved(Path::new(&result.file), wire_path) {
                req.snapshot
                    .map(|c| parse_ignore_rules(&c.ignore))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

        // Diff status
        let mut all_diffs = Vec::new();
        if saved.status != current.status {
            all_diffs.push(wire_core::diff::DiffEntry {
                path: "status".to_string(),
                kind: wire_core::diff::DiffKind::Changed,
                old: Some(serde_json::json!(saved.status)),
                new: Some(serde_json::json!(current.status)),
            });
        }

        // Diff headers
        for (k, v) in &saved.headers {
            match current.headers.get(k) {
                Some(cv) if cv != v => {
                    all_diffs.push(wire_core::diff::DiffEntry {
                        path: format!("headers.{k}"),
                        kind: wire_core::diff::DiffKind::Changed,
                        old: Some(serde_json::json!(v)),
                        new: Some(serde_json::json!(cv)),
                    });
                }
                None => {
                    all_diffs.push(wire_core::diff::DiffEntry {
                        path: format!("headers.{k}"),
                        kind: wire_core::diff::DiffKind::Removed,
                        old: Some(serde_json::json!(v)),
                        new: None,
                    });
                }
                _ => {}
            }
        }
        for (k, v) in &current.headers {
            if !saved.headers.contains_key(k) {
                all_diffs.push(wire_core::diff::DiffEntry {
                    path: format!("headers.{k}"),
                    kind: wire_core::diff::DiffKind::Added,
                    old: None,
                    new: Some(serde_json::json!(v)),
                });
            }
        }

        // Diff body
        let body_diffs = structural_diff(&saved.body, &current.body);
        let body_diffs: Vec<_> = body_diffs
            .into_iter()
            .map(|mut d| {
                d.path = if d.path.is_empty() {
                    "body".to_string()
                } else {
                    format!("body.{}", d.path)
                };
                d
            })
            .collect();

        all_diffs.extend(body_diffs);

        // Apply ignore rules
        let filtered = wire_core::diff::ignore::filter_diffs(all_diffs, &ignore_rules);

        if filtered.is_empty() {
            println!(
                "\n{} {} snapshot matches",
                "\u{2713}".green().bold(),
                result.name
            );
        } else {
            has_diffs = true;
            println!(
                "\n{} {} snapshot differs:",
                "\u{2717}".red().bold(),
                result.name
            );
            println!("{}", format_diff(&filtered));
        }
    }

    if has_diffs {
        1
    } else {
        0
    }
}

fn print_test_results(summary: &runner::TestRunSummary) {
    if summary.results.is_empty() {
        println!("{}", "No tests found.".dimmed());
        return;
    }

    for result in &summary.results {
        let status_icon = if result.all_passed() {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };

        let method_colored = match result.method.as_str() {
            "GET" => result.method.green(),
            "POST" => result.method.yellow(),
            "PUT" => result.method.blue(),
            "PATCH" => result.method.magenta(),
            "DELETE" => result.method.red(),
            _ => result.method.normal(),
        };

        println!(
            "{} {} {} {}",
            status_icon,
            method_colored,
            result.name,
            result.file.dimmed()
        );

        if let Some(ref err) = result.error {
            println!("    {} {}", "ERROR:".red().bold(), err);
            continue;
        }

        for assertion in &result.assertions {
            let icon = if assertion.passed {
                "  ✓".green()
            } else {
                "  ✗".red()
            };
            print!(
                "  {} {} {} {}",
                icon,
                assertion.field.cyan(),
                assertion.operator.dimmed(),
                assertion.expected
            );
            if !assertion.passed {
                print!(" {} {}", "(got".dimmed(), assertion.actual.red());
                print!("{}", ")".dimmed());
            }
            println!();
        }
    }

    println!();
    let total = format!(
        "{} assertions, {} passed, {} failed",
        summary.total_assertions, summary.passed, summary.failed
    );
    if summary.all_passed() {
        println!("{} {}", "✓".green().bold(), total.green());
    } else {
        println!("{} {}", "✗".red().bold(), total.red());
    }
    if summary.errors > 0 {
        println!(
            "  {} {} request(s) failed to execute",
            "⚠".yellow(),
            summary.errors
        );
    }
}

fn cmd_list(dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wire_path = Path::new(dir);
    if !wire_path.is_dir() {
        return Err(format!("Directory not found: {dir}").into());
    }

    let collection = load_collection(wire_path)?;

    println!(
        "{} {} (v{})",
        "Collection:".bold(),
        collection.metadata.name,
        collection.metadata.version,
    );

    if !collection.environments.is_empty() {
        println!(
            "{} {}",
            "Environments:".dimmed(),
            collection
                .environments
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some(active) = &collection.metadata.active_env {
            println!("{} {}", "Active:".dimmed(), active.green());
        }
    }

    println!();
    println!("{}", "Requests:".bold());
    if collection.requests.is_empty() {
        println!("  {}", "(none)".dimmed());
    } else {
        for (path, req) in &collection.requests {
            let relative = path
                .strip_prefix(wire_path)
                .unwrap_or(path)
                .to_string_lossy();
            let method_colored = match req.method.as_str() {
                "GET" => req.method.green(),
                "POST" => req.method.yellow(),
                "PUT" => req.method.blue(),
                "PATCH" => req.method.magenta(),
                "DELETE" => req.method.red(),
                _ => req.method.normal(),
            };
            println!("  {} {} — {}", method_colored, req.name, relative.dimmed());
        }
    }

    Ok(())
}

fn cmd_drift(project_dir: &str, wire_dir: &str, fix: bool, output: &str) -> i32 {
    let project_path = Path::new(project_dir);
    let wire_path = Path::new(wire_dir);

    // Scan the codebase
    let scan_result = match wire_core::scan::scan_project(project_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", "Error".red().bold());
            return 1;
        }
    };

    // Load the collection
    let collection = if wire_path.is_dir() {
        match load_collection(wire_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: {e}", "Error".red().bold());
                return 1;
            }
        }
    } else {
        eprintln!(
            "{}: collection directory not found: {wire_dir}",
            "Error".red().bold()
        );
        return 1;
    };

    let report = drift::compare(&scan_result.endpoints, &collection.requests);

    if output == "json" {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{}: {e}", "Error".red().bold());
                return 1;
            }
        }
    } else {
        print_drift_report(&report);
    }

    // --fix: sync collection to match code
    if fix && report.has_drift() {
        println!();
        println!("{}", "Fixing drift...".bold());

        let requests_dir = wire_path.join("requests");
        let _ = std::fs::create_dir_all(&requests_dir);

        for item in &report.items {
            match item.category {
                drift::DriftCategory::New => {
                    if let Some(ep) = scan_result
                        .endpoints
                        .iter()
                        .find(|ep| ep.method == item.method && ep.route == item.route)
                    {
                        let slug = item
                            .name
                            .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                            .to_lowercase();
                        let group_dir = requests_dir.join(&ep.group);
                        let _ = std::fs::create_dir_all(&group_dir);
                        let mut file_path = group_dir.join(format!("{slug}.wire.yaml"));
                        if file_path.exists() {
                            file_path = group_dir
                                .join(format!("{}-{slug}.wire.yaml", item.method.to_lowercase()));
                        }
                        if !file_path.exists() {
                            let request = wire_core::scan::endpoint_to_request(ep);
                            if let Ok(yaml) = serde_yaml::to_string(&request) {
                                if std::fs::write(&file_path, yaml).is_ok() {
                                    println!(
                                        "  {} {} {}",
                                        "+".green().bold(),
                                        "added".green(),
                                        file_path.display()
                                    );
                                }
                            }
                        }
                    }
                }
                drift::DriftCategory::Changed => {
                    // Update existing request with fresh endpoint data
                    if let Some(ref req_path) = item.request_path {
                        if let Some(ep) = scan_result
                            .endpoints
                            .iter()
                            .find(|ep| ep.method == item.method && ep.route == item.route)
                        {
                            let request = wire_core::scan::endpoint_to_request(ep);
                            if let Ok(yaml) = serde_yaml::to_string(&request) {
                                if std::fs::write(req_path, yaml).is_ok() {
                                    println!(
                                        "  {} {} {}",
                                        "~".yellow().bold(),
                                        "updated".yellow(),
                                        req_path
                                    );
                                }
                            }
                        }
                    }
                }
                drift::DriftCategory::Stale => {
                    // Remove request files for endpoints no longer in code
                    if let Some(ref req_path) = item.request_path {
                        let path = Path::new(req_path);
                        if path.exists() && std::fs::remove_file(path).is_ok() {
                            println!("  {} {} {}", "-".red().bold(), "removed".red(), req_path);
                        }
                    }
                }
            }
        }
    }

    if report.has_drift() {
        1
    } else {
        0
    }
}

const CLAUDE_MD_TRIGGER: &str = "\
# Wire: HTTP API Client for AI Agents — Run /wire for full reference.\n\
\n\
When working in a project with HTTP APIs, use the `wire` CLI tool. This applies whenever you:\n\
- **Send HTTP requests** — use `wire send` instead of curl, httpie, or raw HTTP libraries\n\
- **Create or modify endpoints** — always create/update a `.wire.yaml` request file with test assertions\n\
- **Hit or test an existing endpoint** — if no `.wire.yaml` request file exists for it, create one with tests before sending\n\
- **Test APIs** — use `wire test` to run assertions (status codes, response fields, types, performance)\n\
- **Work with CRUD resources** — create a chain (`wire chain run`) that tests the full create→read→update→delete lifecycle\n\
- **Add related endpoints** — always check if the new endpoint can be chained with existing ones (e.g. create→read→update→delete, auth→use token→refresh)\n\
- **Encounter a project with HTTP endpoints** — generate or maintain a `.wire/` collection so all endpoints are documented and testable\n\
- **Use secrets or tokens** — use secret references ($env:, $dotenv:, $aws:, $vault:) instead of hardcoding values\n\
- **Scan codebases for endpoints** — use `wire generate` to auto-discover routes and build collections\n\
- **Check for API drift** — use `wire drift` to compare source code against the collection\n\
- **Verify response stability** — save snapshots (`wire send --snapshot`) for stable endpoints, then `wire test --snapshot` to catch regressions\n\
\n\
Wire keeps all API documentation, tests, and request definitions local in `.wire/` directories. No cloud, no accounts.";

fn cmd_install_claude_skill() {
    println!("{}", "Installing Wire Claude Code skill".cyan().bold());
    println!();

    let skill_content = include_str!("../../../skills/wire.md");
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let claude_dir = Path::new(&home).join(".claude");
    let commands_dir = claude_dir.join("commands");

    // 1. Install the skill file
    match std::fs::create_dir_all(&commands_dir) {
        Ok(_) => {}
        Err(e) => {
            eprintln!(
                "  {} Failed to create {}: {e}",
                "\u{2717}".red().bold(),
                commands_dir.display()
            );
            return;
        }
    }

    let skill_path = commands_dir.join("wire.md");
    match std::fs::write(&skill_path, skill_content) {
        Ok(_) => {
            println!(
                "  {} Skill installed at {}",
                "\u{2713}".green().bold(),
                skill_path.display()
            );
        }
        Err(e) => {
            eprintln!("  {} Failed to write skill: {e}", "\u{2717}".red().bold());
            return;
        }
    }

    // 2. Append trigger to global CLAUDE.md (if not already present)
    let claude_md_path = claude_dir.join("CLAUDE.md");
    let already_present = claude_md_path
        .exists()
        .then(|| std::fs::read_to_string(&claude_md_path).unwrap_or_default())
        .map(|content| content.contains("# Wire: HTTP API Client for AI Agents"))
        .unwrap_or(false);

    if !already_present {
        // Back up existing CLAUDE.md before modifying (only if no backup exists yet)
        if claude_md_path.exists() {
            let backup_path = claude_dir.join("CLAUDE.md.wire-backup");
            if !backup_path.exists() && std::fs::copy(&claude_md_path, &backup_path).is_ok() {
                println!(
                    "  {} Backed up CLAUDE.md to {}",
                    "\u{2713}".green().bold(),
                    backup_path.display()
                );
            }
        }

        let append = if claude_md_path.exists() {
            format!("\n{CLAUDE_MD_TRIGGER}\n")
        } else {
            format!("{CLAUDE_MD_TRIGGER}\n")
        };

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&claude_md_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                if file.write_all(append.as_bytes()).is_ok() {
                    println!(
                        "  {} Auto-trigger added to {}",
                        "\u{2713}".green().bold(),
                        claude_md_path.display()
                    );
                    println!(
                        "    {}",
                        "Claude will now use Wire automatically for HTTP tasks".dimmed()
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "  {} Failed to update CLAUDE.md: {e}",
                    "\u{2717}".red().bold()
                );
            }
        }
    } else {
        println!(
            "  {} Auto-trigger already present in CLAUDE.md",
            "\u{2713}".green().bold(),
        );
    }

    println!();
    println!("{}", "Done.".green().bold());
}

fn cmd_uninstall_claude_skill() {
    println!("{}", "Removing Wire Claude Code skill".cyan().bold());
    println!();

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let claude_dir = Path::new(&home).join(".claude");

    // 1. Remove the skill file
    let skill_path = claude_dir.join("commands").join("wire.md");
    if skill_path.exists() {
        match std::fs::remove_file(&skill_path) {
            Ok(_) => {
                println!(
                    "  {} Removed skill at {}",
                    "\u{2713}".green().bold(),
                    skill_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "  {} Failed to remove {}: {e}",
                    "\u{2717}".red().bold(),
                    skill_path.display()
                );
            }
        }
    } else {
        println!("  {} Skill already removed", "\u{2713}".green().bold(),);
    }

    // 2. Remove Wire trigger block from global CLAUDE.md
    let claude_md_path = claude_dir.join("CLAUDE.md");
    if claude_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&claude_md_path) {
            // Remove the exact trigger block (with optional leading newline)
            let new_content = content
                .replace(&format!("\n{CLAUDE_MD_TRIGGER}\n"), "\n")
                .replace(&format!("{CLAUDE_MD_TRIGGER}\n"), "");
            let new_content = new_content.trim_end().to_string()
                + if new_content.trim_end().is_empty() {
                    ""
                } else {
                    "\n"
                };
            if new_content.trim().is_empty() {
                // CLAUDE.md is now empty — remove it entirely
                let _ = std::fs::remove_file(&claude_md_path);
                println!("  {} Removed empty CLAUDE.md", "\u{2713}".green().bold(),);
            } else if new_content != content {
                let _ = std::fs::write(&claude_md_path, &new_content);
                println!(
                    "  {} Removed Wire trigger from CLAUDE.md",
                    "\u{2713}".green().bold(),
                );
            }
        }
    }

    println!();
    println!(
        "{}",
        "To fully remove Wire, run: cargo uninstall wire-cli".dimmed()
    );
}

async fn cmd_snapshot_update(
    file: &str,
    env_name: Option<&str>,
    wire_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = load_request_resolved(Path::new(file), Path::new(wire_dir))?;

    let mut scope = VariableScope::new();
    let wire_path = Path::new(wire_dir);
    if wire_path.is_dir() {
        let collection = load_collection(wire_path)?;
        let active_env = env_name
            .map(|s| s.to_string())
            .or(collection.metadata.active_env);
        if let Some(env_key) = &active_env {
            if let Some(environment) = collection.environments.get(env_key) {
                scope.push_layer(environment.variables.clone());
            }
        }
    }

    let client = HttpClient::new()?;

    println!(
        "{} {} {}",
        "\u{2192}".blue().bold(),
        request.method.cyan().bold(),
        request.url
    );

    let response = execute(&client, &request, &scope).await?;

    println!(
        "  {} {}  {} {}ms",
        "Status:".dimmed(),
        response.status,
        "Time:".dimmed(),
        response.elapsed.as_millis()
    );

    let snap = wire_core::snapshot::snapshot_from_response(
        response.status,
        &response.headers,
        &response.body,
    );
    let request_relative = file
        .trim_start_matches(wire_dir)
        .trim_start_matches('/')
        .trim_start_matches('\\');
    let path = wire_core::snapshot::save_snapshot(&snap, Path::new(wire_dir), request_relative)?;

    println!(
        "{} Snapshot updated: {}",
        "\u{2713}".green().bold(),
        path.display()
    );

    Ok(())
}

fn cmd_env_check(wire_dir: &str) -> i32 {
    let wire_path = Path::new(wire_dir);
    if !wire_path.is_dir() {
        eprintln!("{}: directory not found: {wire_dir}", "Error".red().bold());
        return 1;
    }

    let collection = match load_collection(wire_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e}", "Error".red().bold());
            return 1;
        }
    };

    if collection.environments.is_empty() {
        println!("{}", "No environments found.".dimmed());
        return 0;
    }

    // Derive project dir from wire_dir parent (for .env discovery)
    let project_dir = wire_path.parent();

    let results = wire_core::variables::secrets::check_collection_secrets(
        &collection.environments,
        project_dir,
    );

    if results.is_empty() {
        println!(
            "{}",
            "No secret references found in any environment.".dimmed()
        );
        return 0;
    }

    println!(
        "{} {} secret reference(s)",
        "Checking".cyan().bold(),
        results.len()
    );
    println!();

    let mut current_env = String::new();
    let mut failures = 0;

    for result in &results {
        if result.env_name != current_env {
            if !current_env.is_empty() {
                println!();
            }
            println!("  {}", result.env_name.bold());
            current_env = result.env_name.clone();
        }

        if result.resolved {
            println!(
                "    {} {} (${}: {})",
                "\u{2713}".green().bold(),
                result.var_name,
                result.source,
                result.key.dimmed(),
            );
        } else {
            failures += 1;
            println!(
                "    {} {} (${}: {})",
                "\u{2717}".red().bold(),
                result.var_name,
                result.source,
                result.key.dimmed(),
            );
            if let Some(ref err) = result.error {
                println!("      {}", err.red());
            }
        }
    }

    println!();
    let passed = results.len() - failures;
    if failures == 0 {
        println!(
            "{} All {} secret(s) resolved successfully",
            "\u{2713}".green().bold(),
            passed
        );
        0
    } else {
        println!(
            "{} {}/{} secret(s) failed to resolve",
            "\u{2717}".red().bold(),
            failures,
            results.len()
        );
        1
    }
}

fn cmd_generate(project_dir: &str, output: Option<&str>) -> i32 {
    let project_path = Path::new(project_dir);
    let output_path = output.map(Path::new).unwrap_or(project_path);

    println!("{} {}", "Scanning".cyan().bold(), project_path.display());

    let (scan, collection) =
        match wire_core::scan::scan_and_create_collection(project_path, output_path) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("{}: {e}", "Error".red().bold());
                return 1;
            }
        };

    let framework = match scan.framework {
        wire_core::scan::types::Framework::AspNet => "ASP.NET",
        wire_core::scan::types::Framework::Express => "Express",
        wire_core::scan::types::Framework::NextJs => "Next.js",
        wire_core::scan::types::Framework::SpringBoot => "Spring Boot",
        wire_core::scan::types::Framework::FastApi => "FastAPI",
        wire_core::scan::types::Framework::Unknown => "Unknown",
    };

    println!(
        "  {} {} files scanned",
        "✓".green().bold(),
        scan.files_scanned,
    );
    println!("  {} Framework: {}", "✓".green().bold(), framework.cyan(),);
    println!(
        "  {} {} endpoints discovered",
        "✓".green().bold(),
        scan.endpoints.len(),
    );

    if scan.endpoints.is_empty() {
        println!();
        println!("{}", "No endpoints found. No collection created.".yellow());
        return 0;
    }

    let col = match &collection {
        Some(c) => c,
        None => {
            eprintln!(
                "{}: endpoints found but collection was not created",
                "Error".red().bold()
            );
            return 1;
        }
    };

    let wire_dir = output_path.join(".wire");
    println!();
    println!(
        "{} {} at {}",
        "✓".green().bold(),
        format!("Collection \"{}\" created", col.metadata.name).green(),
        wire_dir.display().to_string().dimmed(),
    );

    // Show grouped endpoints
    let mut groups: std::collections::BTreeMap<
        &str,
        Vec<&wire_core::scan::types::DiscoveredEndpoint>,
    > = std::collections::BTreeMap::new();
    for ep in &scan.endpoints {
        groups.entry(&ep.group).or_default().push(ep);
    }

    println!();
    println!("{}", "Endpoints:".bold());
    for (group, endpoints) in &groups {
        println!("  {}/", group.cyan());
        for ep in endpoints {
            let method_colored = match ep.method.as_str() {
                "GET" => ep.method.green(),
                "POST" => ep.method.yellow(),
                "PUT" => ep.method.blue(),
                "PATCH" => ep.method.magenta(),
                "DELETE" => ep.method.red(),
                _ => ep.method.normal(),
            };
            println!("    {} {} — {}", method_colored, ep.route, ep.name.dimmed());
        }
    }

    println!();
    println!(
        "{}",
        format!(
            "{} request(s) in {} group(s)",
            scan.endpoints.len(),
            groups.len()
        )
        .dimmed()
    );

    0
}

async fn cmd_chain_run(file: &str, env_name: Option<&str>, wire_dir: &str) -> i32 {
    let wire_path = Path::new(wire_dir);

    // Load the request file
    let request = match load_request(Path::new(file)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", "Error".red().bold());
            return 1;
        }
    };

    if request.chain.is_empty() {
        eprintln!(
            "{}: no chain section found in {}",
            "Error".red().bold(),
            file
        );
        return 1;
    }

    // Build variable scope from environment
    let mut scope = VariableScope::new();
    let mut resolved_env_name: Option<String> = None;
    if wire_path.is_dir() {
        if let Ok(collection) = load_collection(wire_path) {
            let active_env = env_name
                .map(|s| s.to_string())
                .or(collection.metadata.active_env);

            if let Some(env_key) = &active_env {
                if let Some(environment) = collection.environments.get(env_key) {
                    scope.push_layer(environment.variables.clone());
                    resolved_env_name = Some(env_key.clone());
                } else {
                    eprintln!(
                        "{}: environment '{}' not found",
                        "Warning".yellow().bold(),
                        env_key,
                    );
                }
            }
        }
    }

    let client = match HttpClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e}", "Error".red().bold());
            return 1;
        }
    };

    println!(
        "{} {} ({} steps)",
        "Running chain".cyan().bold(),
        request.name,
        request.chain.len()
    );
    println!();

    // Show chain steps before running
    println!("{}", "Steps:".dimmed());
    for (i, step) in request.chain.iter().enumerate() {
        let extracts = if step.extract.is_empty() {
            String::new()
        } else {
            let vars: Vec<&str> = step.extract.keys().map(|s| s.as_str()).collect();
            format!(" {} {}", "\u{2192}".dimmed(), vars.join(", ").dimmed())
        };
        println!(
            "  {} {}{}",
            format!("{}.", i + 1).dimmed(),
            step.run.cyan(),
            extracts
        );
    }
    println!();

    let result = chain::execute_chain(
        &request.chain,
        wire_path,
        &scope,
        &client,
        resolved_env_name.as_deref(),
    )
    .await;

    // Print step results
    println!("{}", "Results:".dimmed());
    for step in &result.steps {
        let icon = if step.passed {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };

        let status_colored = if step.status == 0 {
            "---".dimmed()
        } else if step.status < 300 {
            format!("{}", step.status).green()
        } else {
            format!("{}", step.status).red()
        };

        println!(
            "  {} Step {} — {} [{}] {}ms",
            icon,
            step.step_index + 1,
            step.request_name,
            status_colored,
            step.elapsed_ms,
        );

        // Show request details
        if !step.request_method.is_empty() {
            let method_colored = match step.request_method.as_str() {
                "GET" => step.request_method.green(),
                "POST" => step.request_method.yellow(),
                "PUT" => step.request_method.blue(),
                "PATCH" => step.request_method.magenta(),
                "DELETE" => step.request_method.red(),
                _ => step.request_method.normal(),
            };
            println!(
                "      {} {} {}",
                "→".dimmed(),
                method_colored,
                step.request_url
            );
        }

        // Show response body preview
        if !step.response_body.is_empty() {
            let preview =
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&step.response_body) {
                    let pretty =
                        serde_json::to_string_pretty(&json).unwrap_or(step.response_body.clone());
                    if pretty.lines().count() > 6 {
                        pretty.lines().take(6).collect::<Vec<_>>().join("\n") + "\n      ..."
                    } else {
                        pretty
                    }
                } else if step.response_body.len() > 200 {
                    format!("{}...", &step.response_body[..197])
                } else {
                    step.response_body.clone()
                };
            for (i, line) in preview.lines().enumerate() {
                if i == 0 {
                    println!("      {} {}", "←".dimmed(), line.dimmed());
                } else {
                    println!("        {}", line.dimmed());
                }
            }
        }

        // Show extracted variables
        if !step.extracted.is_empty() {
            for (var, val) in &step.extracted {
                let display_val = if val.len() > 60 {
                    format!("{}...", &val[..57])
                } else {
                    val.clone()
                };
                println!(
                    "      {} {} = {}",
                    "↳".dimmed(),
                    var.cyan(),
                    display_val.dimmed()
                );
            }
        }

        if let Some(ref err) = step.error {
            println!("      {} {}", "ERROR:".red().bold(), err);
        }

        println!();
    }

    println!();
    if result.success {
        println!(
            "{} {} steps completed in {}ms",
            "✓".green().bold(),
            result.steps.len(),
            result.total_elapsed_ms,
        );
        0
    } else {
        println!(
            "{} {}",
            "✗".red().bold(),
            result.error.as_deref().unwrap_or("Chain failed"),
        );
        1
    }
}

fn print_drift_report(report: &drift::DriftReport) {
    if !report.has_drift() {
        println!("{}", "No drift detected.".green().bold());
        return;
    }

    for item in &report.items {
        let (icon, color_label) = match item.category {
            drift::DriftCategory::New => ("+".green().bold(), "NEW".green()),
            drift::DriftCategory::Stale => ("-".red().bold(), "STALE".red()),
            drift::DriftCategory::Changed => ("~".yellow().bold(), "CHANGED".yellow()),
        };

        let method_colored = match item.method.as_str() {
            "GET" => item.method.green(),
            "POST" => item.method.yellow(),
            "PUT" => item.method.blue(),
            "PATCH" => item.method.magenta(),
            "DELETE" => item.method.red(),
            _ => item.method.normal(),
        };

        println!(
            "  {} [{}] {} {} — {}",
            icon, color_label, method_colored, item.route, item.name
        );

        for change in &item.changes {
            println!("      {} {}", "↳".dimmed(), change);
        }
        if let Some(ref path) = item.request_path {
            println!("      {} {}", "file:".dimmed(), path.dimmed());
        }
    }

    println!();
    let summary = format!(
        "{} new, {} stale, {} changed",
        report.new_count, report.stale_count, report.changed_count
    );
    println!("{} {}", "Drift:".bold(), summary);
}

fn cmd_template_list(dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wire_path = Path::new(dir);
    if !wire_path.is_dir() {
        return Err(format!("Directory not found: {dir}").into());
    }

    let names = list_templates(wire_path)?;

    if names.is_empty() {
        println!("{}", "No templates found.".dimmed());
        println!(
            "  {}",
            "Add templates in .wire/templates/<name>.wire.yaml".dimmed()
        );
        return Ok(());
    }

    println!("{}", "Templates:".bold());
    for name in &names {
        println!("  {}", name.cyan());
    }
    println!();
    println!("{}", format!("{} template(s)", names.len()).dimmed());

    Ok(())
}

fn cmd_history(
    action: Option<HistoryAction>,
    limit: usize,
    wire_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let wire_path = Path::new(wire_dir);
    let history_path = if wire_path.is_dir() {
        history::resolve_history_path(Some(wire_path))
    } else {
        history::resolve_history_path(None)
    };

    match action {
        Some(HistoryAction::Clear) => {
            history::clear_history(&history_path)?;
            println!("{}", "History cleared.".green());
        }
        None => {
            let entries = history::load_history(&history_path, limit)?;
            if entries.is_empty() {
                println!("{}", "No history entries.".dimmed());
                return Ok(());
            }

            println!("{}", "Request History:".bold());
            println!();
            for entry in &entries {
                let method_colored = match entry.method.as_str() {
                    "GET" => entry.method.green(),
                    "POST" => entry.method.yellow(),
                    "PUT" => entry.method.blue(),
                    "PATCH" => entry.method.magenta(),
                    "DELETE" => entry.method.red(),
                    _ => entry.method.normal(),
                };
                let status_colored = if entry.status < 300 {
                    format!("{}", entry.status).green()
                } else if entry.status < 400 {
                    format!("{}", entry.status).yellow()
                } else {
                    format!("{}", entry.status).red()
                };
                let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M:%S");
                println!(
                    "  {} {} {} — {} {}ms",
                    method_colored,
                    entry.url,
                    status_colored,
                    timestamp.to_string().dimmed(),
                    entry.elapsed_ms,
                );
            }
            println!();
            println!(
                "{}",
                format!("{} entries (showing last {limit})", entries.len()).dimmed()
            );
        }
    }

    Ok(())
}

fn print_status(status: u16) {
    let status_str = format!("{status}");
    let colored = if status < 300 {
        status_str.green().bold()
    } else if status < 400 {
        status_str.yellow().bold()
    } else {
        status_str.red().bold()
    };
    print!("  {} {}", "Status:".dimmed(), colored);
}
