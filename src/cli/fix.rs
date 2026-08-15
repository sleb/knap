use std::path::{Path, PathBuf};

use lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    TextDocumentEdit, WorkspaceEdit,
};

use crate::config::Config;
use crate::handlers::slug;
use crate::index::{NoteIndex, ResolvedLink};
use crate::{config, edit, handlers, index};

/// A single fix `knap fix` decided to make, paired with a human-readable
/// description for `--dry-run` output and the applied-fixes summary.
/// `pub(crate)` (not just this module's) since `knap lint --fix` plans and
/// applies the identical fixes through `plan_fixes`/`apply` below.
pub(crate) struct PlannedFix {
    edit: WorkspaceEdit,
    pub(crate) description: String,
}

/// `config::for_path` → `index::build`, mirroring `lint`'s target selection.
/// Works in absolute paths throughout — unlike diagnostics, a fix's
/// `WorkspaceEdit` carries real URIs (`path_to_uri` panics on a relative
/// path), so `path` is absolutized up front the same way `rename-file`/
/// `rename-heading`/`rename-tag` already do. Delegates the actual fix
/// computation to `plan_fixes` (shared with `knap lint --fix`), then either
/// prints the plan (`--dry-run`) or applies it and prints a summary.
pub fn run(path: &Path, dry_run: bool) -> anyhow::Result<()> {
    let path_abs = absolute(path)?;
    let (idx, config, targets) = targets_for(&path_abs)?;

    let fixes = plan_fixes(&idx, &config, &targets);

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

    let touched = apply(&fixes)?;
    println!("applied {} fix(es) in {touched} file(s)", fixes.len());
    for fix in &fixes {
        println!("{}", fix.description);
    }

    Ok(())
}

/// Builds the index and fix targets for `path_abs` (already absolutized): a
/// file path scopes to just that note, a directory scopes to every indexed
/// note — the setup `fix::run` and `knap apply`'s `fix` operation both need
/// before calling `plan_fixes`.
pub(crate) fn targets_for(path_abs: &Path) -> anyhow::Result<(NoteIndex, Config, Vec<PathBuf>)> {
    let config = config::for_path(path_abs, None, &[])?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions, &[])?;
    let targets: Vec<PathBuf> = if path_abs.is_file() {
        vec![path_abs.to_path_buf()]
    } else {
        idx.all_notes().map(|n| n.path.clone()).collect()
    };
    Ok((idx, config, targets))
}

/// For every link in every note in `targets`, computes the fix
/// `compute_link_fix`+`suggest_link_fix` (repoint to the one unambiguous
/// closest-matching note) or, failing that, `compute_create_missing_file_fix`
/// (stub it out) would make for a broken link, and
/// `compute_anchor_fix`+`suggest_anchor_fix` would make for a broken anchor
/// — skipping anything ambiguous in either case. Shared by `fix::run` and
/// `knap lint --fix`, so both apply exactly the same unambiguous-only
/// contract; `knap lint --suggest` surfaces the same ranked candidates for
/// the ambiguous cases this leaves alone, so an agent can pick one and edit
/// by hand instead of guessing blind.
pub(crate) fn plan_fixes(idx: &NoteIndex, config: &Config, targets: &[PathBuf]) -> Vec<PlannedFix> {
    let mut fixes: Vec<PlannedFix> = Vec::new();
    for target in targets {
        let Some(note) = idx.get_note(target) else {
            continue;
        };
        for link in &note.md_links {
            if link.target.is_empty() {
                continue;
            }
            match idx.resolve(&note.path, &link.target) {
                ResolvedLink::Broken => {
                    if let Some(new_target) =
                        handlers::suggest_link_fix(&link.target, &link.text, &note.path, idx)
                    {
                        fixes.push(PlannedFix {
                            edit: handlers::compute_link_fix(
                                &note.path,
                                link.target_range,
                                &new_target,
                            ),
                            description: format!(
                                "{}: repoint '{}' → '{new_target}'",
                                note.path.display(),
                                link.target
                            ),
                        });
                    } else {
                        fixes.push(PlannedFix {
                            edit: handlers::compute_create_missing_file_fix(
                                link, &note.path, config,
                            ),
                            description: format!("create {}", link.target),
                        });
                    }
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
                    let Some(heading) =
                        handlers::suggest_anchor_fix(&slug(anchor), &link.text, target_note)
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
    fixes
}

/// Merges `fixes` into one `WorkspaceEdit` and applies it, returning the
/// number of files touched. A no-op (`Ok(0)`, no filesystem access) when
/// `fixes` is empty — callers don't need to check first.
pub(crate) fn apply(fixes: &[PlannedFix]) -> anyhow::Result<usize> {
    if fixes.is_empty() {
        return Ok(0);
    }
    let merged = merge_fixes(fixes);
    edit::apply(&merged)
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn targets_for_file_path_returns_single_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        std::fs::write(root.join("b.md"), "# B\n").unwrap();

        let path_abs = index::normalize_path(&root.join("a.md"));
        let (_idx, _config, targets) = targets_for(&path_abs).unwrap();

        assert_eq!(targets, vec![path_abs]);
    }

    #[test]
    fn targets_for_directory_path_returns_all_notes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        std::fs::write(root.join("b.md"), "# B\n").unwrap();
        std::fs::write(root.join("c.md"), "# C\n").unwrap();

        let path_abs = index::normalize_path(root);
        let (_idx, _config, targets) = targets_for(&path_abs).unwrap();

        let got: HashSet<PathBuf> = targets.into_iter().collect();
        let want: HashSet<PathBuf> = ["a.md", "b.md", "c.md"]
            .into_iter()
            .map(|name| index::normalize_path(&root.join(name)))
            .collect();
        assert_eq!(got, want);
    }
}
