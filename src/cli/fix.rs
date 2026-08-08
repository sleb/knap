use std::path::{Path, PathBuf};

use lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    TextDocumentEdit, WorkspaceEdit,
};

use crate::handlers::slug;
use crate::index::ResolvedLink;
use crate::{config, edit, handlers, index};

/// A single fix `knap fix` decided to make, paired with a human-readable
/// description for `--dry-run` output and the applied-fixes summary.
struct PlannedFix {
    edit: WorkspaceEdit,
    description: String,
}

/// `config::for_path` → `index::build`, mirroring `lint`'s target selection.
/// Works in absolute paths throughout — unlike diagnostics, a fix's
/// `WorkspaceEdit` carries real URIs (`path_to_uri` panics on a relative
/// path), so `path` is absolutized up front the same way `rename-file`/
/// `rename-heading`/`rename-tag` already do. For every link in every target
/// note, computes the fix `compute_create_missing_file_fix` or
/// `compute_anchor_fix`+`suggest_anchor_fix` would make — skipping anything
/// ambiguous — then either prints the plan (`--dry-run`) or merges every
/// fix's edit into one `WorkspaceEdit` and hands it to `edit::apply`.
pub fn run(path: &Path, dry_run: bool) -> anyhow::Result<()> {
    let path_abs = absolute(path)?;
    let config = config::for_path(&path_abs, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    let targets: Vec<PathBuf> = if path_abs.is_file() {
        vec![path_abs.clone()]
    } else {
        idx.all_notes().map(|n| n.path.clone()).collect()
    };

    let mut fixes: Vec<PlannedFix> = Vec::new();
    for target in &targets {
        let Some(note) = idx.get_note(target) else {
            continue;
        };
        for link in &note.md_links {
            if link.target.is_empty() {
                continue;
            }
            match idx.resolve(&note.path, &link.target) {
                ResolvedLink::Broken => {
                    fixes.push(PlannedFix {
                        edit: handlers::compute_create_missing_file_fix(link, &note.path, &config),
                        description: format!("create {}", link.target),
                    });
                }
                ResolvedLink::Found(target_path) => {
                    let (Some(anchor), Some(anchor_range)) = (&link.anchor, link.anchor_range)
                    else {
                        continue;
                    };
                    let Some(target_note) = idx.get_note(&target_path) else {
                        continue;
                    };
                    let anchor_matches = target_note
                        .headings
                        .iter()
                        .any(|h| slug(&h.text) == slug(anchor));
                    if anchor_matches {
                        continue;
                    }
                    let Some(heading) = handlers::suggest_anchor_fix(&slug(anchor), target_note)
                    else {
                        continue;
                    };
                    let new_anchor = slug(&heading.text);
                    fixes.push(PlannedFix {
                        edit: handlers::compute_anchor_fix(&note.path, anchor_range, &new_anchor),
                        description: format!(
                            "{}: anchor '#{anchor}' → '#{new_anchor}'",
                            note.path.display()
                        ),
                    });
                }
            }
        }
    }

    if fixes.is_empty() {
        println!("no safe fixes found");
        return Ok(());
    }

    if dry_run {
        for fix in &fixes {
            println!("would {}", fix.description);
        }
        return Ok(());
    }

    let merged = merge_fixes(&fixes);
    let touched = edit::apply(&merged)?;
    println!("applied {} fix(es) in {touched} file(s)", fixes.len());
    for fix in &fixes {
        println!("{}", fix.description);
    }

    Ok(())
}

/// Merges every `PlannedFix.edit` into one `document_changes` list — a
/// `compute_anchor_fix` result is `changes`-shaped (one `(uri,
/// Vec<TextEdit>)` map entry) and gets wrapped into a
/// `DocumentChangeOperation::Edit`, the same conversion `rename-file` already
/// does in `src/cli/rename.rs`; a `compute_create_missing_file_fix` result is
/// already `document_changes`-shaped, so its operations are appended as-is.
fn merge_fixes(fixes: &[PlannedFix]) -> WorkspaceEdit {
    let mut ops: Vec<DocumentChangeOperation> = Vec::new();
    for fix in fixes {
        if let Some(changes) = &fix.edit.changes {
            for (uri, edits) in changes {
                ops.push(DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: None,
                    },
                    edits: edits.iter().cloned().map(OneOf::Left).collect(),
                }));
            }
        }
        if let Some(DocumentChanges::Operations(fix_ops)) = &fix.edit.document_changes {
            ops.extend(fix_ops.iter().cloned());
        }
    }
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(ops)),
        ..Default::default()
    }
}

/// Resolves `path` to an absolute, normalized location. Same helper
/// `src/cli/rename.rs` defines for the same reason — `path_to_uri` requires
/// an absolute path.
fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(index::normalize_path(&joined))
}
