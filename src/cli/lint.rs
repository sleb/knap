use std::path::{Path, PathBuf};

use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde::Serialize;

use crate::{config, handlers, index};

#[derive(Serialize)]
struct LintReport {
    diagnostics: Vec<FileDiagnostics>,
    problem_count: usize,
    file_count: usize,
}

#[derive(Serialize)]
struct FileDiagnostics {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
}

/// `config::for_path` → `index::build` → `handlers::compute_diagnostics` per
/// target file — no new diagnostic logic here, just CLI-output shaping.
pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    let config = config::for_path(path, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    let targets: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        let mut notes: Vec<PathBuf> = idx.all_notes().map(|n| n.path.clone()).collect();
        notes.sort();
        notes
    };

    let mut files: Vec<FileDiagnostics> = targets
        .into_iter()
        .filter_map(|target| {
            let diagnostics = handlers::compute_diagnostics(&target, &idx, &config);
            (!diagnostics.is_empty()).then_some(FileDiagnostics { path: target, diagnostics })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let problem_count: usize = files.iter().map(|f| f.diagnostics.len()).sum();
    let file_count = files.len();

    if json {
        let report = LintReport { diagnostics: files, problem_count, file_count };
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
            }
        }
        println!();
        println!("{problem_count} problem(s) in {file_count} file(s)");
    }

    if problem_count > 0 {
        anyhow::bail!("{problem_count} problem(s) found");
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
