//! Edit Applicator: executes a computed `WorkspaceEdit` against files on
//! disk.
//!
//! Handlers in `src/handlers.rs` compute `WorkspaceEdit`s but never write
//! anything — over LSP, the client applies the edit. Headless CLI commands
//! have no client, so this module is that missing piece. It's a top-level
//! module rather than something under `src/cli/` because it isn't
//! rename-specific: any `WorkspaceEdit` a handler emits (e.g. v0.13's `knap
//! fix` against the "create missing file" quick fix) can be applied here.

use std::fs;

use lsp_types::{DocumentChangeOperation, DocumentChanges, OneOf, ResourceOp, TextEdit, WorkspaceEdit};

use crate::handlers::uri_to_path;
use crate::parser::LineIndex;

/// Applies every change in `edit` to the filesystem and returns the number
/// of files touched across both `changes` and `document_changes`.
// Not yet called outside tests — `knap rename-file` (v0.12 step 5) is its
// first caller.
#[allow(dead_code)]
pub(crate) fn apply(edit: &WorkspaceEdit) -> anyhow::Result<usize> {
    let mut touched = 0;

    if let Some(changes) = &edit.changes {
        for (uri, edits) in changes {
            apply_text_edits(uri, edits)?;
            touched += 1;
        }
    }

    if let Some(DocumentChanges::Operations(ops)) = &edit.document_changes {
        for op in ops {
            match op {
                DocumentChangeOperation::Edit(text_document_edit) => {
                    let edits: Vec<TextEdit> = text_document_edit
                        .edits
                        .iter()
                        .map(|e| match e {
                            OneOf::Left(edit) => edit.clone(),
                            OneOf::Right(annotated) => annotated.text_edit.clone(),
                        })
                        .collect();
                    apply_text_edits(&text_document_edit.text_document.uri, &edits)?;
                    touched += 1;
                }
                DocumentChangeOperation::Op(ResourceOp::Rename(rename)) => {
                    let old_path = uri_to_path(&rename.old_uri).ok_or_else(|| {
                        anyhow::anyhow!("rename: not a file URI: {}", rename.old_uri.as_str())
                    })?;
                    let new_path = uri_to_path(&rename.new_uri).ok_or_else(|| {
                        anyhow::anyhow!("rename: not a file URI: {}", rename.new_uri.as_str())
                    })?;
                    fs::rename(old_path, new_path)?;
                    touched += 1;
                }
                DocumentChangeOperation::Op(ResourceOp::Create(create)) => {
                    let path = uri_to_path(&create.uri).ok_or_else(|| {
                        anyhow::anyhow!("create: not a file URI: {}", create.uri.as_str())
                    })?;
                    let ignore_if_exists = create
                        .options
                        .as_ref()
                        .and_then(|o| o.ignore_if_exists)
                        .unwrap_or(false);
                    if !(path.exists() && ignore_if_exists) {
                        fs::write(&path, "")?;
                    }
                    touched += 1;
                }
                DocumentChangeOperation::Op(ResourceOp::Delete(_)) => {
                    anyhow::bail!("edit::apply: delete not supported");
                }
            }
        }
    }

    Ok(touched)
}

/// Applies `edits` to the file at `uri`, sorted by descending
/// `(line, character)` of `range.start` so earlier edits' offsets aren't
/// invalidated by later ones landing first.
fn apply_text_edits(uri: &lsp_types::Uri, edits: &[TextEdit]) -> anyhow::Result<()> {
    let path = uri_to_path(uri)
        .ok_or_else(|| anyhow::anyhow!("edit: not a file URI: {}", uri.as_str()))?;
    let mut content = fs::read_to_string(&path)?;

    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.range.start));

    for edit in sorted {
        let line_index = LineIndex::new(&content);
        let start = line_index.offset(edit.range.start);
        let end = line_index.offset(edit.range.end);
        content.replace_range(start..end, &edit.new_text);
    }

    fs::write(&path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lsp_types::{
        CreateFile, CreateFileOptions, DeleteFile, OptionalVersionedTextDocumentIdentifier,
        Position, Range, RenameFile, TextDocumentEdit,
    };
    use tempfile::tempdir;

    use super::*;
    use crate::handlers::path_to_uri;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(start: (u32, u32), end: (u32, u32)) -> Range {
        Range {
            start: pos(start.0, start.1),
            end: pos(end.0, end.1),
        }
    }

    #[test]
    fn apply_changes_single_edit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "hello world\n").unwrap();

        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(
                path_to_uri(&path),
                vec![TextEdit {
                    range: range((0, 6), (0, 11)),
                    new_text: "there".to_string(),
                }],
            )])),
            ..Default::default()
        };

        let touched = apply(&edit).unwrap();
        assert_eq!(touched, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello there\n");
    }

    #[test]
    fn apply_changes_multiple_edits_same_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "one two three\n").unwrap();

        // Listed out of source order — descending application must still
        // land both correctly.
        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(
                path_to_uri(&path),
                vec![
                    TextEdit {
                        range: range((0, 8), (0, 13)),
                        new_text: "3".to_string(),
                    },
                    TextEdit {
                        range: range((0, 0), (0, 3)),
                        new_text: "1".to_string(),
                    },
                ],
            )])),
            ..Default::default()
        };

        apply(&edit).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "1 two 3\n");
    }

    #[test]
    fn apply_changes_multiple_files() {
        let dir = tempdir().unwrap();
        let path_a = dir.path().join("a.md");
        let path_b = dir.path().join("b.md");
        fs::write(&path_a, "aaa\n").unwrap();
        fs::write(&path_b, "bbb\n").unwrap();

        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([
                (
                    path_to_uri(&path_a),
                    vec![TextEdit {
                        range: range((0, 0), (0, 3)),
                        new_text: "xxx".to_string(),
                    }],
                ),
                (
                    path_to_uri(&path_b),
                    vec![TextEdit {
                        range: range((0, 0), (0, 3)),
                        new_text: "yyy".to_string(),
                    }],
                ),
            ])),
            ..Default::default()
        };

        let touched = apply(&edit).unwrap();
        assert_eq!(touched, 2);
        assert_eq!(fs::read_to_string(&path_a).unwrap(), "xxx\n");
        assert_eq!(fs::read_to_string(&path_b).unwrap(), "yyy\n");
    }

    #[test]
    fn apply_changes_missing_file_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.md");

        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(
                path_to_uri(&path),
                vec![TextEdit {
                    range: range((0, 0), (0, 0)),
                    new_text: "x".to_string(),
                }],
            )])),
            ..Default::default()
        };

        assert!(apply(&edit).is_err());
    }

    #[test]
    fn apply_document_changes_edit_then_rename_in_order() {
        let dir = tempdir().unwrap();
        let old_path = dir.path().join("old.md");
        let new_path = dir.path().join("new.md");
        fs::write(&old_path, "hello world\n").unwrap();

        let ops = vec![
            DocumentChangeOperation::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: path_to_uri(&old_path),
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: range((0, 6), (0, 11)),
                    new_text: "there".to_string(),
                })],
            }),
            DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile {
                old_uri: path_to_uri(&old_path),
                new_uri: path_to_uri(&new_path),
                options: None,
                annotation_id: None,
            })),
        ];

        let edit = WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(ops)),
            ..Default::default()
        };

        let touched = apply(&edit).unwrap();
        assert_eq!(touched, 2);
        assert!(!old_path.exists());
        assert_eq!(fs::read_to_string(&new_path).unwrap(), "hello there\n");
    }

    #[test]
    fn apply_document_changes_create() {
        let dir = tempdir().unwrap();
        let new_path = dir.path().join("new.md");
        let existing_path = dir.path().join("existing.md");
        fs::write(&existing_path, "keep me\n").unwrap();

        let ops = vec![
            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                uri: path_to_uri(&new_path),
                options: None,
                annotation_id: None,
            })),
            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                uri: path_to_uri(&existing_path),
                options: Some(CreateFileOptions {
                    ignore_if_exists: Some(true),
                    overwrite: None,
                }),
                annotation_id: None,
            })),
        ];

        let edit = WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(ops)),
            ..Default::default()
        };

        let touched = apply(&edit).unwrap();
        assert_eq!(touched, 2);
        assert_eq!(fs::read_to_string(&new_path).unwrap(), "");
        assert_eq!(fs::read_to_string(&existing_path).unwrap(), "keep me\n");
    }

    #[test]
    fn apply_document_changes_delete_errors_not_implemented() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "x\n").unwrap();

        let ops = vec![DocumentChangeOperation::Op(ResourceOp::Delete(
            DeleteFile {
                uri: path_to_uri(&path),
                options: None,
            },
        ))];

        let edit = WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(ops)),
            ..Default::default()
        };

        assert!(apply(&edit).is_err());
        // No filesystem change — the file must still exist.
        assert!(path.exists());
    }
}
