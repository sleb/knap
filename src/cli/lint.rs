use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use serde::Serialize;

use crate::cli::{FailOn, hint};
use crate::{config, handlers, index};

#[derive(Serialize)]
struct LintReport {
    diagnostics: Vec<FileDiagnostics>,
    problem_count: usize,
    file_count: usize,
    blocking_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
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
/// diagnostic's `data` field.
pub fn run(
    path: &Path,
    json: bool,
    fail_on: FailOn,
    since: Option<&str>,
    suggest_n: usize,
    exclude: &[String],
    ignore_link_target: &[String],
) -> anyhow::Result<()> {
    let config = config::for_path(path, None, exclude, ignore_link_target)?;
    let (idx, _) = index::build(&config.index_roots, &config.path_filter)?;

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

    let hint = hint_for_files(&files);

    if json {
        let report = LintReport {
            diagnostics: files,
            problem_count,
            file_count,
            blocking_count,
            hint,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
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
                if let Some(data) = &d.data {
                    print_suggestions(data);
                }
            }
        }
        println!();
        println!("{problem_count} problem(s) in {file_count} file(s)");
        if let Some(hint) = &hint {
            println!("hint: {hint}");
        }
        if blocking_count != problem_count {
            println!("{blocking_count} at or above --fail-on threshold");
        }
    }

    if blocking_count > 0 {
        anyhow::bail!("{blocking_count} problem(s) found");
    }

    Ok(())
}

/// Prints each ranked suggestion in `data.suggestions` (as attached by
/// `compute_diagnostics_with_suggestions`) as an indented line under the
/// diagnostic that owns it, plus a one-line note when `data.text_mismatch`
/// is set and another when `data.same_basename` is set. No-op if `data` has
/// no `suggestions` array — i.e. `--suggest` wasn't passed, or this
/// diagnostic isn't a broken-link/broken-anchor.
fn print_suggestions(data: &serde_json::Value) {
    let Some(suggestions) = data.get("suggestions").and_then(|s| s.as_array()) else {
        return;
    };
    for s in suggestions {
        let target = s.get("target").and_then(|t| t.as_str()).unwrap_or("?");
        let distance = s
            .get("distance")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        match s.get("text_distance").and_then(serde_json::Value::as_u64) {
            Some(td) => println!("    -> {target} (distance {distance}, text distance {td})"),
            None => println!("    -> {target} (distance {distance})"),
        }
    }
    if data
        .get("text_mismatch")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        println!(
            "    (top match by distance differs from best text match — verify before applying)"
        );
    }
    if data
        .get("same_basename")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        println!(
            "    (top suggestions share a filename — path ranking alone can't tell them apart; verify by directory or content)"
        );
    }
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

/// Computes `LintReport.hint` from the diagnostic `code`s across `files`:
/// `has_repointable` if any is `broken-link`/`broken-anchor`, `has_manual` if
/// any is one of the four frontmatter codes.
fn hint_for_files(files: &[FileDiagnostics]) -> Option<String> {
    let mut has_repointable = false;
    let mut has_manual = false;
    for code in files
        .iter()
        .flat_map(|f| &f.diagnostics)
        .filter_map(|d| d.code.as_ref())
    {
        let NumberOrString::String(code) = code else {
            continue;
        };
        match code.as_str() {
            handlers::CODE_BROKEN_LINK | handlers::CODE_BROKEN_ANCHOR => has_repointable = true,
            handlers::CODE_MISSING_FRONTMATTER
            | handlers::CODE_MISSING_REQUIRED_FIELD
            | handlers::CODE_INVALID_FIELD_VALUE
            | handlers::CODE_UNKNOWN_FIELD => has_manual = true,
            _ => {}
        }
        if has_repointable && has_manual {
            break;
        }
    }
    hint::lint(has_repointable, has_manual)
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

    #[test]
    fn lint_report_hint_absent_when_clean() {
        let files: Vec<FileDiagnostics> = Vec::new();
        let report = LintReport {
            hint: hint_for_files(&files),
            diagnostics: files,
            problem_count: 0,
            file_count: 0,
            blocking_count: 0,
        };
        let json = serde_json::to_value(&report).expect("serializes");
        assert!(json.get("hint").is_none());
    }

    #[test]
    fn lint_report_hint_present_for_broken_link() {
        let diagnostic = Diagnostic {
            code: Some(NumberOrString::String(
                crate::handlers::CODE_BROKEN_LINK.to_string(),
            )),
            ..Diagnostic::new_simple(lsp_types::Range::default(), "broken link".to_string())
        };
        let files = vec![FileDiagnostics {
            path: PathBuf::from("note.md"),
            diagnostics: vec![diagnostic],
        }];
        let report = LintReport {
            hint: hint_for_files(&files),
            problem_count: 1,
            file_count: files.len(),
            blocking_count: 1,
            diagnostics: files,
        };
        let json = serde_json::to_value(&report).expect("serializes");
        let hint = json
            .get("hint")
            .and_then(|h| h.as_str())
            .expect("hint key present");
        assert!(hint.contains("apply"));
    }
}
