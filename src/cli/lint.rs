use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde::Serialize;

use crate::cli::FailOn;
use crate::{config, handlers, index};

#[derive(Serialize)]
struct LintReport {
    diagnostics: Vec<FileDiagnostics>,
    problem_count: usize,
    file_count: usize,
    blocking_count: usize,
    /// Present only when `--fix` was passed: every fix `knap fix` would have
    /// made, already applied to disk before the diagnostics above were
    /// computed — so `diagnostics` reflects the post-fix state, not what was
    /// true when the command started.
    #[serde(skip_serializing_if = "Option::is_none")]
    fixes_applied: Option<Vec<String>>,
}

#[derive(Serialize)]
struct FileDiagnostics {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
}

/// `config::for_path` → `index::build` → `handlers::compute_diagnostics` per
/// target file — no new diagnostic logic here, just CLI-output shaping.
/// `suggest_n > 0` switches to `compute_diagnostics_with_suggestions`, which
/// attaches ranked candidate fixes to each broken-link/broken-anchor
/// diagnostic's `data` field instead of leaving `knap fix` as the only way
/// to see them. `fix` collapses the usual "lint, fix, lint again" sequence
/// into one call: plans and applies every safe fix over the whole `path`
/// root — the same scope bare `knap fix` uses, not narrowed by `--since`,
/// since a fix elsewhere in the vault (e.g. a rename's target) can resolve a
/// diagnostic in a file that wasn't itself edited — via `cli::fix`'s
/// `plan_fixes`/`apply`, the identical logic `knap fix` runs. That pass
/// needs absolute paths (`path_to_uri` requires them, same reason
/// `cli::fix::run` absolutizes up front), so it runs against its own
/// absolutized index rather than the possibly-relative one built above;
/// after applying, the report's own index is rebuilt from disk so what's
/// reported reflects the post-fix state, not the stale pre-fix one.
pub fn run(
    path: &Path,
    json: bool,
    fail_on: FailOn,
    since: Option<&str>,
    suggest_n: usize,
    fix: bool,
) -> anyhow::Result<()> {
    let config = config::for_path(path, None, &[])?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (mut idx, _) = index::build(&config.index_roots, &extensions);

    let mut fixes_applied = None;
    if fix {
        let abs_path = absolute(path)?;
        let fix_config = config::for_path(&abs_path, None, &[])?;
        let fix_extensions: Vec<&str> = fix_config.extensions.iter().map(String::as_str).collect();
        let (fix_idx, _) = index::build(&fix_config.index_roots, &fix_extensions);
        let fix_targets: Vec<PathBuf> = if abs_path.is_file() {
            vec![abs_path]
        } else {
            fix_idx.all_notes().map(|n| n.path.clone()).collect()
        };
        let planned = super::fix::plan_fixes(&fix_idx, &fix_config, &fix_targets);
        fixes_applied = Some(planned.iter().map(|f| f.description.clone()).collect());
        super::fix::apply(&planned)?;
        // Files on disk changed underneath `idx` — rebuild so the
        // diagnostics computed below reflect the post-fix state.
        idx = index::build(&config.index_roots, &extensions).0;
    }

    let mut targets: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        let mut notes: Vec<PathBuf> = idx.all_notes().map(|n| n.path.clone()).collect();
        notes.sort();
        notes
    };

    if let Some(git_ref) = since {
        let changed = changed_paths_since(&config.index_roots[0], git_ref)?;
        targets.retain(|t| {
            t.canonicalize()
                .map(|c| changed.contains(&c))
                .unwrap_or(false)
        });
    }

    let mut files: Vec<FileDiagnostics> = targets
        .into_iter()
        .filter_map(|target| {
            let diagnostics =
                handlers::compute_diagnostics_with_suggestions(&target, &idx, &config, suggest_n);
            (!diagnostics.is_empty()).then_some(FileDiagnostics {
                path: target,
                diagnostics,
            })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let problem_count: usize = files.iter().map(|f| f.diagnostics.len()).sum();
    let file_count = files.len();
    let blocking_count: usize = files
        .iter()
        .flat_map(|f| &f.diagnostics)
        .filter(|d| severity_rank(d.severity) <= fail_on.rank())
        .count();

    if json {
        let report = LintReport {
            diagnostics: files,
            problem_count,
            file_count,
            blocking_count,
            fixes_applied,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if let Some(descriptions) = &fixes_applied {
            if descriptions.is_empty() {
                println!("no safe fixes found");
            } else {
                println!("applied {} fix(es)", descriptions.len());
                for d in descriptions {
                    println!("{d}");
                }
            }
            println!();
        }
        for file in &files {
            for d in &file.diagnostics {
                println!(
                    "{}:{}:{}: {}: {}",
                    file.path.display(),
                    d.range.start.line + 1,
                    d.range.start.character + 1,
                    severity_label(d.severity),
                    d.message,
                );
            }
        }
        println!();
        println!("{problem_count} problem(s) in {file_count} file(s)");
        if blocking_count != problem_count {
            println!("{blocking_count} at or above --fail-on threshold");
        }
    }

    if blocking_count > 0 {
        anyhow::bail!("{blocking_count} problem(s) found");
    }

    Ok(())
}

fn severity_label(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "warning",
    }
}

/// Union of tracked changes (`git diff --name-only <git_ref>`) and untracked
/// new files (`git ls-files --others --exclude-standard`), each resolved to
/// a canonicalized absolute path so it can be matched against `NoteIndex`
/// paths regardless of the CLI's own relative/absolute working directory.
fn changed_paths_since(root: &Path, git_ref: &str) -> anyhow::Result<HashSet<PathBuf>> {
    let repo_root = git_output(root, &["rev-parse", "--show-toplevel"])
        .context("--since requires a git repository")?;
    let repo_root = PathBuf::from(repo_root.trim());

    let mut changed = HashSet::new();
    for rel in git_output(&repo_root, &["diff", "--name-only", git_ref])?.lines() {
        changed.insert(repo_root.join(rel));
    }
    for rel in git_output(&repo_root, &["ls-files", "--others", "--exclude-standard"])?.lines() {
        changed.insert(repo_root.join(rel));
    }
    Ok(changed
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect())
}

fn git_output(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Resolves `path` to an absolute, normalized location. Same helper
/// `src/cli/fix.rs` and `src/cli/rename.rs` define for the same reason —
/// `path_to_uri` requires an absolute path.
fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(index::normalize_path(&joined))
}

fn severity_rank(severity: Option<DiagnosticSeverity>) -> i32 {
    match severity {
        Some(DiagnosticSeverity::ERROR) => 1,
        Some(DiagnosticSeverity::INFORMATION) => 3,
        Some(DiagnosticSeverity::HINT) => 4,
        _ => 2, // WARNING, and None — matches severity_label's existing default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_rank_orders_error_above_warning_above_info_above_hint() {
        let error = severity_rank(Some(DiagnosticSeverity::ERROR));
        let warning = severity_rank(Some(DiagnosticSeverity::WARNING));
        let none = severity_rank(None);
        let info = severity_rank(Some(DiagnosticSeverity::INFORMATION));
        let hint = severity_rank(Some(DiagnosticSeverity::HINT));

        assert!(error < warning);
        assert_eq!(warning, none);
        assert!(warning < info);
        assert!(info < hint);
    }

    #[test]
    fn fail_on_default_matches_todays_behavior() {
        // Every diagnostic compute_diagnostics emits today is WARNING (rank 2).
        // FailOn::Warning must admit it.
        assert!(severity_rank(Some(DiagnosticSeverity::WARNING)) <= FailOn::Warning.rank());
        assert!(severity_rank(None) <= FailOn::Warning.rank());
    }
}
