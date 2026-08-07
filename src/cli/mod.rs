use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod check;
mod index;
mod lint;
mod lsp;
mod parse;
mod version;

#[derive(Parser)]
#[command(
    name = "knap",
    version,
    about = "A minimal, opinionated LSP for Markdown notes"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the LSP server on stdio.
    Lsp,
    /// Check a directory (or single file) for problems.
    Lint {
        /// File or directory to lint. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Emit machine-readable JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Build and print the note index for a directory.
    Index {
        /// Directory to index.
        path: PathBuf,
        /// Emit machine-readable JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Parse a single file and print its structure.
    Parse {
        /// File to parse.
        path: PathBuf,
    },
    /// Run an in-process LSP smoke test.
    Check,
    /// Print the version.
    Version,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Lsp => lsp::run(),
        Commands::Lint { path, json } => lint::run(&path, json),
        Commands::Index { path, json } => index::run(&path, json),
        Commands::Parse { path } => parse::run(&path),
        Commands::Check => check::run(),
        Commands::Version => {
            version::run();
            Ok(())
        }
    }
}
