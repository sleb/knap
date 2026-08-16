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
/// Thin wrapper around `rename_file_at`, scoped to the process's actual cwd.
pub fn run_file(old: &Path, new: &Path) -> anyhow::Result<()> {
    let cwd = absolute(Path::new("."))?;
    let touched = rename_file_at(&cwd, old, new)?;
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
/// Thin wrapper around `rename_heading_at`, scoped to the process's actual
/// cwd.
pub fn run_heading(file: &Path, old: &str, new: &str) -> anyhow::Result<()> {
    let cwd = absolute(Path::new("."))?;
    let touched = rename_heading_at(&cwd, file, old, new)?;
    println!(
        "{old:?} → {new:?} in {} ({touched} file(s) touched)",
        file.display()
    );

    Ok(())
}

/// `knap rename-tag <old> <new>`: rewrites every frontmatter occurrence of a
/// tag across the workspace, atomically.
///
/// Thin wrapper around `rename_tag_at`, scoped to the process's actual cwd.
pub fn run_tag(old: &str, new: &str) -> anyhow::Result<()> {
    let cwd = absolute(Path::new("."))?;
    let touched = rename_tag_at(&cwd, old, new)?;
    println!("#{old} → #{new} ({touched} file(s) touched)");

    Ok(())
}

/// Core of `rename-file`, scoped to `root` instead of always the process's
/// actual cwd. `old`/`new` are resolved against `root` when relative
/// (`Path::join` leaves an already-absolute argument unchanged). Shared with
/// `knap apply`, which calls this once per `rename-file` entry in a batch,
/// with `root` pointing at that batch's scratch copy of the workspace.
///
/// Reuses `handlers::handle_will_rename_files` — the same edit computation
/// the LSP `workspace/willRenameFiles` handler gives an editor — then wraps
/// its `changes`-shaped output into `document_changes` with a trailing
/// `Op(ResourceOp::Rename)`, since headlessly nothing else is going to
/// perform the actual move (an LSP client does that itself after the
/// pre-rename request returns).
pub(crate) fn rename_file_at(root: &Path, old: &Path, new: &Path) -> anyhow::Result<usize> {
    let old_abs = index::normalize_path(&root.join(old));
    let new_abs = index::normalize_path(&root.join(new));
    anyhow::ensure!(old_abs.exists(), "{}: no such file", old.display());
    anyhow::ensure!(!new_abs.exists(), "{}: already exists", new.display());

    // Index off the absolute path, not `old` as given — `index::build` walks
    // its roots and stores whatever path shape it started from, and that
    // shape must match `old_uri`/`new_uri` below (also absolute) or
    // `handle_will_rename_files`'s `index.links_to(&old_path)` lookup misses
    // every incoming link.
    let old_uri = path_to_uri(&old_abs);
    let new_uri = path_to_uri(&new_abs);

    // Scope off `root`, not `old_abs` — `config::for_path` treats a *file*
    // argument's parent directory as the whole index root, which would
    // silently drop every note outside `old`'s own directory (and any
    // incoming link living there) from the index.
    let config = config::for_path(root, None, &[])?;
    let (idx, _) = index::build(&config.index_roots, &config.path_filter)?;

    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: old_uri.as_str().to_string(),
            new_uri: new_uri.as_str().to_string(),
        }],
    };

    let link_edit = handlers::handle_will_rename_files(params, &idx);
    let wrapped = wrap_as_document_changes(link_edit, old_uri, new_uri);

    edit::apply(&wrapped)
}

/// Core of `rename-heading`, scoped to `root` instead of always the
/// process's actual cwd. `file` is resolved against `root` when relative.
/// Shared with `knap apply`, which calls this once per `rename-heading`
/// entry in a batch, with `root` pointing at that batch's scratch copy of
/// the workspace.
///
/// Reuses `handlers::find_heading` to turn `old` (text or slug) into a
/// `Heading`, and `handlers::compute_heading_rename` for the edit — the same
/// computation the LSP `rename` handler uses for a heading under the
/// cursor. No resource op is involved (the file doesn't move), so the
/// resulting `WorkspaceEdit` is handed to `edit::apply` unwrapped.
pub(crate) fn rename_heading_at(
    root: &Path,
    file: &Path,
    old: &str,
    new: &str,
) -> anyhow::Result<usize> {
    let file_abs = index::normalize_path(&root.join(file));
    anyhow::ensure!(file_abs.exists(), "{}: no such file", file.display());

    // Scope off `root`, not `file_abs` — see the matching comment in
    // `rename_file_at`: a file argument would otherwise narrow the index to
    // just `file`'s own directory, missing cross-file anchor links from
    // anywhere else in the vault.
    let config = config::for_path(root, None, &[])?;
    let (idx, _) = index::build(&config.index_roots, &config.path_filter)?;

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
    edit::apply(&edit)
}

/// Core of `rename-tag`, scoped to `root` instead of always the process's
/// actual cwd. Shared with `knap apply`, which calls this once per
/// `rename-tag` entry in a batch, with `root` pointing at that batch's
/// scratch copy of the workspace.
///
/// No cursor, so no current-file/disk-fallback special case — every note
/// carrying the tag comes from the index.
pub(crate) fn rename_tag_at(root: &Path, old: &str, new: &str) -> anyhow::Result<usize> {
    // `config::for_path` stores whatever root it's given as `index_roots`
    // verbatim; a relative `root` would make every `note.path` relative too,
    // and `compute_tag_rename`'s `path_to_uri` requires absolute paths (same
    // constraint `rename_file_at` works around by absolutizing before
    // indexing).
    let config = config::for_path(root, None, &[])?;
    let (idx, _) = index::build(&config.index_roots, &config.path_filter)?;

    anyhow::ensure!(
        idx.notes_by_tag(old).next().is_some(),
        "no note uses tag {old:?}"
    );

    let edit = handlers::compute_tag_rename(old, new, &idx);
    edit::apply(&edit)
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

    #[test]
    fn rename_file_at_scopes_to_given_root_not_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("old.md"), "Link to [sibling](sibling.md).\n").unwrap();
        std::fs::write(root.join("sibling.md"), "# Sibling\n").unwrap();
        std::fs::write(root.join("linker.md"), "Link to [old](old.md).\n").unwrap();

        // The process's real cwd (the crate root this test binary runs
        // from) has none of these files, so this only succeeds if
        // `rename_file_at` resolves everything against `root`.
        let touched = rename_file_at(root, Path::new("old.md"), Path::new("new.md")).unwrap();

        assert!(!root.join("old.md").exists());
        assert!(root.join("new.md").exists());
        let linker = std::fs::read_to_string(root.join("linker.md")).unwrap();
        assert!(
            linker.contains("(new.md)"),
            "incoming link not rewritten: {linker}"
        );
        assert!(touched >= 2, "expected at least the move + linker edit");
    }

    #[test]
    fn rename_file_at_new_path_exists_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("old.md"), "old\n").unwrap();
        std::fs::write(root.join("new.md"), "already here\n").unwrap();

        let err = rename_file_at(root, Path::new("old.md"), Path::new("new.md")).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "unexpected error: {err}"
        );
        // Nothing touched: `old.md` still there, `new.md` untouched.
        assert!(root.join("old.md").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("new.md")).unwrap(),
            "already here\n"
        );
    }

    #[test]
    fn rename_heading_at_scopes_to_given_root_not_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("a.md"),
            "# Old Section\n\nSee the [self link](#old-section).\n",
        )
        .unwrap();
        std::fs::write(
            root.join("b.md"),
            "See the [cross-file link](a.md#old-section).\n",
        )
        .unwrap();

        // As above: only reachable if `root`, not the real cwd, is used to
        // build the index and resolve `file`.
        let touched =
            rename_heading_at(root, Path::new("a.md"), "Old Section", "New Section").unwrap();

        let a = std::fs::read_to_string(root.join("a.md")).unwrap();
        assert!(a.contains("# New Section"), "heading not renamed: {a}");
        assert!(
            a.contains("(#new-section)"),
            "same-file anchor not rewritten: {a}"
        );

        let b = std::fs::read_to_string(root.join("b.md")).unwrap();
        assert!(
            b.contains("(a.md#new-section)"),
            "cross-file anchor not rewritten: {b}"
        );
        assert!(touched >= 2, "expected edits to both a.md and b.md");
    }

    #[test]
    fn rename_tag_at_scopes_to_given_root_not_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.md"), "---\ntags: draft\n---\n\n# A\n").unwrap();
        std::fs::write(root.join("b.md"), "---\ntags: [draft, rust]\n---\n\n# B\n").unwrap();

        // Only reachable if `root`, not the real cwd, is indexed.
        let touched = rename_tag_at(root, "draft", "published").unwrap();

        let a = std::fs::read_to_string(root.join("a.md")).unwrap();
        assert!(a.contains("tags: published"), "bare scalar: {a}");
        let b = std::fs::read_to_string(root.join("b.md")).unwrap();
        assert!(b.contains("tags: [published, rust]"), "inline list: {b}");
        assert_eq!(touched, 2);
    }
}
