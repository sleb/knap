use std::path::{Path, PathBuf};

use lsp_types::{
    DocumentChangeOperation, DocumentChanges, FileRename, OneOf,
    OptionalVersionedTextDocumentIdentifier, RenameFile, RenameFilesParams, ResourceOp,
    TextDocumentEdit, WorkspaceEdit,
};

use crate::handlers::path_to_uri;
use crate::{config, edit, handlers, index};

/// `knap rename-file <old> <new>`: moves a note on disk and rewrites every
/// incoming and outgoing Markdown link affected by the move.
///
/// Reuses `handlers::handle_will_rename_files` — the same edit computation
/// the LSP `workspace/willRenameFiles` handler gives an editor — then wraps
/// its `changes`-shaped output into `document_changes` with a trailing
/// `Op(ResourceOp::Rename)`, since headlessly nothing else is going to
/// perform the actual move (an LSP client does that itself after the
/// pre-rename request returns).
pub fn run_file(old: &Path, new: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(old.exists(), "{}: no such file", old.display());
    anyhow::ensure!(!new.exists(), "{}: already exists", new.display());

    // Index off the absolute path, not `old` as given — `index::build` walks
    // its roots and stores whatever path shape it started from, and that
    // shape must match `old_uri`/`new_uri` below (also absolute) or
    // `handle_will_rename_files`'s `index.links_to(&old_path)` lookup misses
    // every incoming link.
    let old_abs = absolute(old)?;
    let new_abs = absolute(new)?;
    let old_uri = path_to_uri(&old_abs);
    let new_uri = path_to_uri(&new_abs);

    let config = config::for_path(&old_abs, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: old_uri.as_str().to_string(),
            new_uri: new_uri.as_str().to_string(),
        }],
    };

    let link_edit = handlers::handle_will_rename_files(params, &idx);
    let wrapped = wrap_as_document_changes(link_edit, old_uri, new_uri);

    let touched = edit::apply(&wrapped)?;
    println!(
        "{} → {} ({touched} file(s) touched)",
        old.display(),
        new.display()
    );

    Ok(())
}

/// `knap rename-heading <file> <old> <new>`: rewrites a heading's text and
/// every anchor link that targets it, same-file and cross-file alike.
///
/// Reuses `handlers::find_heading` to turn `<old>` (text or slug) into a
/// `Heading`, and `handlers::compute_heading_rename` for the edit — the same
/// computation the LSP `rename` handler uses for a heading under the
/// cursor. No resource op is involved (the file doesn't move), so the
/// resulting `WorkspaceEdit` is handed to `edit::apply` unwrapped.
pub fn run_heading(file: &Path, old: &str, new: &str) -> anyhow::Result<()> {
    anyhow::ensure!(file.exists(), "{}: no such file", file.display());

    let file_abs = absolute(file)?;
    let config = config::for_path(&file_abs, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    let disk_note;
    let note: &crate::parser::Note = match idx.get_note(&file_abs) {
        Some(n) => n,
        None => {
            let content = std::fs::read_to_string(&file_abs)?;
            disk_note = crate::parser::parse(&file_abs, &content);
            &disk_note
        }
    };

    let heading = handlers::find_heading(note, old)
        .ok_or_else(|| anyhow::anyhow!("{}: no heading matching {old:?}", file.display()))?;

    let edit = handlers::compute_heading_rename(&file_abs, note, heading, new, &idx);
    let touched = edit::apply(&edit)?;
    println!("{old:?} → {new:?} in {} ({touched} file(s) touched)", file.display());

    Ok(())
}

/// `knap rename-tag <old> <new>`: rewrites every frontmatter occurrence of a
/// tag across the workspace, atomically.
///
/// No cursor, so no current-file/disk-fallback special case — every note
/// carrying the tag comes from the index. `config::for_path(".")` scopes the
/// workspace root the same way `knap lint`/`knap index` default to the
/// current directory.
pub fn run_tag(old: &str, new: &str) -> anyhow::Result<()> {
    // `config::for_path` stores whatever root it's given as `index_roots`
    // verbatim; a relative "." would make every `note.path` relative too,
    // and `compute_tag_rename`'s `path_to_uri` requires absolute paths (same
    // constraint `run_file` works around by absolutizing before indexing).
    let cwd = absolute(Path::new("."))?;
    let config = config::for_path(&cwd, None)?;
    let extensions: Vec<&str> = config.extensions.iter().map(String::as_str).collect();
    let (idx, _) = index::build(&config.index_roots, &extensions);

    anyhow::ensure!(
        idx.notes_by_tag(old).next().is_some(),
        "no note uses tag {old:?}"
    );

    let edit = handlers::compute_tag_rename(old, new, &idx);
    let touched = edit::apply(&edit)?;
    println!("#{old} → #{new} ({touched} file(s) touched)");

    Ok(())
}

/// Resolves `path` to an absolute, normalized location without requiring it
/// to exist (`Path::canonicalize` would fail for `new`, which by contract
/// doesn't exist yet).
fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(index::normalize_path(&joined))
}

/// Wraps `link_edit` (the `changes`-shaped output of
/// `handle_will_rename_files`) into `document_changes`, appending the
/// `Op(ResourceOp::Rename)` that actually performs the move — mirroring the
/// construction `handle_code_actions` already uses for its "create missing
/// file" quick fix (`src/handlers.rs`).
fn wrap_as_document_changes(
    link_edit: WorkspaceEdit,
    old_uri: lsp_types::Uri,
    new_uri: lsp_types::Uri,
) -> WorkspaceEdit {
    let mut ops: Vec<DocumentChangeOperation> = link_edit
        .changes
        .into_iter()
        .flatten()
        .map(|(uri, edits)| {
            DocumentChangeOperation::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                edits: edits.into_iter().map(OneOf::Left).collect(),
            })
        })
        .collect();

    ops.push(DocumentChangeOperation::Op(ResourceOp::Rename(
        RenameFile {
            old_uri,
            new_uri,
            options: None,
            annotation_id: None,
        },
    )));

    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(ops)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{DocumentChangeOperation, DocumentChanges, ResourceOp};

    use super::*;
    use crate::handlers::{self, path_to_uri};
    use crate::index::NoteIndex;
    use crate::test_helpers::note;

    #[test]
    fn wrap_rename_file_edit_appends_rename_op() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", ""));
        idx.seed(note("/vault/b.md", "[link](a.md)"));

        let old_uri = path_to_uri(Path::new("/vault/a.md"));
        let new_uri = path_to_uri(Path::new("/vault/sub/a.md"));
        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: old_uri.as_str().to_string(),
                new_uri: new_uri.as_str().to_string(),
            }],
        };

        let link_edit = handlers::handle_will_rename_files(params, &idx);
        let wrapped = wrap_as_document_changes(link_edit, old_uri.clone(), new_uri.clone());

        let Some(DocumentChanges::Operations(ops)) = wrapped.document_changes else {
            panic!("expected document_changes with operations");
        };

        // Every edit op precedes the trailing rename op.
        let (last, rest) = ops.split_last().expect("at least the rename op");
        assert!(
            rest.iter()
                .all(|op| matches!(op, DocumentChangeOperation::Edit(_))),
            "all but the last op should be edits: {rest:?}"
        );
        match last {
            DocumentChangeOperation::Op(ResourceOp::Rename(rename)) => {
                assert_eq!(rename.old_uri, old_uri);
                assert_eq!(rename.new_uri, new_uri);
            }
            other => panic!("expected trailing Op(ResourceOp::Rename), got {other:?}"),
        }

        // b.md's incoming link edit made it into the operations list.
        let has_b_edit = rest.iter().any(|op| {
            matches!(
                op,
                DocumentChangeOperation::Edit(te)
                    if te.text_document.uri == path_to_uri(Path::new("/vault/b.md"))
            )
        });
        assert!(has_b_edit, "expected an edit for b.md's incoming link");
    }
}
