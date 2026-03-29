use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::Path;
use wire_core::collection::{list_templates, load_collection, load_request_resolved};
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
enum TemplateAction {
    /// List all available templates
    List {
        /// Path to .wire directory
        #[arg(default_value = ".wire")]
        dir: String,
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
        } => {
            if let Err(e) = cmd_send(&file, env.as_deref(), &wire_dir).await {
                eprintln!("{}: {e}", "Error".red().bold());
                std::process::exit(1);
            }
        }
        Commands::Test {
            path,
            env,
            wire_dir,
            output,
        } => {
            let exit_code = cmd_test(&path, env.as_deref(), &wire_dir, &output).await;
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

    Ok(())
}

async fn cmd_test(path: &str, env_name: Option<&str>, wire_dir: &str, output: &str) -> i32 {
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

    if summary.all_passed() {
        0
    } else {
        1
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
                    // Add missing request files
                    let slug = item
                        .name
                        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                        .to_lowercase();
                    let file_path = requests_dir.join(format!("{slug}.wire.yaml"));
                    if file_path.exists() {
                        // Try with method prefix to avoid collision
                        let file_path = requests_dir
                            .join(format!("{}-{slug}.wire.yaml", item.method.to_lowercase()));
                        if file_path.exists() {
                            continue;
                        }
                        if let Some(ep) = scan_result
                            .endpoints
                            .iter()
                            .find(|ep| ep.method == item.method && ep.route == item.route)
                        {
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
                    } else if let Some(ep) = scan_result
                        .endpoints
                        .iter()
                        .find(|ep| ep.method == item.method && ep.route == item.route)
                    {
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
