use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod check;
mod index;
mod lint;
mod lsp;
mod parse;
mod rename;
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
    /// Move a note, rewriting incoming and outgoing links.
    RenameFile {
        /// Existing path of the note.
        old: PathBuf,
        /// New path for the note. Must not already exist.
        new: PathBuf,
    },
    /// Rename a heading in a note, rewriting its text and every anchor link
    /// that targets it (same-file and cross-file).
    RenameHeading {
        /// File containing the heading.
        file: PathBuf,
        /// Existing heading text or GFM slug.
        old: String,
        /// New heading text.
        new: String,
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
        Commands::RenameFile { old, new } => rename::run_file(&old, &new),
        Commands::RenameHeading { file, old, new } => rename::run_heading(&file, &old, &new),
        Commands::Check => check::run(),
        Commands::Version => {
            version::run();
            Ok(())
        }
    }
}
