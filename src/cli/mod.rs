use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod check;
mod fix;
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

/// Severity threshold for `knap lint --fail-on`.
#[derive(clap::ValueEnum, Clone, Copy)]
#[value(rename_all = "lower")]
pub enum FailOn {
    Error,
    Warning,
    Info,
    Hint,
}

impl FailOn {
    pub fn rank(self) -> i32 {
        match self {
            FailOn::Error => 1,
            FailOn::Warning => 2,
            FailOn::Info => 3,
            FailOn::Hint => 4,
        }
    }
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
        /// Minimum severity that causes a non-zero exit.
        #[arg(long, value_enum, default_value = "warning")]
        fail_on: FailOn,
        /// Only lint files changed since this git ref (tracked diffs plus
        /// untracked new files). Requires a git repository.
        #[arg(long)]
        since: Option<String>,
        /// Attach up to N ranked candidate fixes to each broken-link or
        /// broken-anchor diagnostic (as `data.suggestions` in --json output),
        /// closest match first. Bare `--suggest` defaults to 3; omit to skip.
        #[arg(long, num_args = 0..=1, default_missing_value = "3", value_name = "N")]
        suggest: Option<usize>,
        /// Apply every safe fix (same as `knap fix`) before reporting, so
        /// the diagnostics shown are what's left after fixing rather than
        /// what was true when the command started. Collapses the usual
        /// lint → fix → lint-again sequence into one call. Mutates files on
        /// disk — this is the one case where `lint` isn't read-only.
        #[arg(long)]
        fix: bool,
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
    /// Rename a tag, rewriting every frontmatter occurrence across the
    /// workspace.
    RenameTag {
        /// Existing tag name.
        old: String,
        /// New tag name.
        new: String,
    },
    /// Run an in-process LSP smoke test.
    Check,
    /// Print the version.
    Version,
    /// Apply safe quick fixes (create missing files, resolve unambiguous
    /// broken anchors) across a directory or a single file.
    Fix {
        /// File or directory to fix. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Print the planned fixes without changing anything on disk.
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Lsp => lsp::run(),
        Commands::Lint {
            path,
            json,
            fail_on,
            since,
            suggest,
            fix,
        } => lint::run(
            &path,
            json,
            fail_on,
            since.as_deref(),
            suggest.unwrap_or(0),
            fix,
        ),
        Commands::Index { path, json } => index::run(&path, json),
        Commands::Parse { path } => parse::run(&path),
        Commands::RenameFile { old, new } => rename::run_file(&old, &new),
        Commands::RenameHeading { file, old, new } => rename::run_heading(&file, &old, &new),
        Commands::RenameTag { old, new } => rename::run_tag(&old, &new),
        Commands::Check => check::run(),
        Commands::Version => {
            version::run();
            Ok(())
        }
        Commands::Fix { path, dry_run } => fix::run(&path, dry_run),
    }
}
