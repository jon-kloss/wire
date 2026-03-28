use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::Path;
use wire_core::collection::{load_collection, load_request};
use wire_core::http::{execute, HttpClient};
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
        Commands::List { dir } => {
            if let Err(e) = cmd_list(&dir) {
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
    // Load the request
    let request = load_request(Path::new(file))?;

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

    Ok(())
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
