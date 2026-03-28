use clap::{Parser, Subcommand};

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
        Commands::Send { file, env } => {
            let env_msg = env
                .as_deref()
                .map(|e| format!(" with env '{e}'"))
                .unwrap_or_default();
            println!("wire send: {file}{env_msg} (not yet implemented)");
        }
        Commands::List { dir } => {
            println!("wire list: {dir} (not yet implemented)");
        }
    }
}
