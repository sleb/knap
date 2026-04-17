// Steps 6–9: completion, definition, references, and diagnostics.
// See docs/design/components/handlers.md

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use lsp_server::{Message, Notification};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Diagnostic, DiagnosticSeverity,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, Location,
    Position, PublishDiagnosticsParams, Range, ReferenceParams, RenameFilesParams, SymbolInformation,
    SymbolKind, TextEdit, WorkspaceEdit, WorkspaceSymbolParams,
};

use crate::index::{NoteIndex, ResolvedLink};

// ─── URI utilities ────────────────────────────────────────────────────────────

// ─── Diagnostics ──────────────────────────────────────────────────────────────

pub fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic> {
    let Some(note) = index.get_note(path) else {
        return vec![];
    };

    note.wiki_links
        .iter()
        .filter_map(|link| match index.resolve(&link.stem) {
            ResolvedLink::Broken => Some(Diagnostic {
                range: link.inner_range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!("Link target not found: '[[{}]]'", link.stem),
                source: Some("knap".to_string()),
                ..Default::default()
            }),
            ResolvedLink::Ambiguous(paths) => Some(Diagnostic {
                range: link.inner_range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!(
                    "'[[{}]]' matches multiple files: {}",
                    link.stem,
                    paths
                        .iter()
                        .map(|p| p.file_name().unwrap_or_default().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                source: Some("knap".to_string()),
                ..Default::default()
            }),
            ResolvedLink::Found(target_path) => link.anchor.as_ref().and_then(|anchor| {
                let target_note = index.get_note(&target_path)?;
                let found = target_note
                    .headings
                    .iter()
                    .any(|h| h.text.to_lowercase() == anchor.to_lowercase());
                if found {
                    return None;
                }
                Some(Diagnostic {
                    range: link.inner_range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!(
                        "Heading not found: '#{}' in '[[{}#{}]]'",
                        anchor, link.stem, anchor
                    ),
                    source: Some("knap".to_string()),
                    ..Default::default()
                })
            }),
        })
        .collect()
}

pub fn publish_diagnostics(paths: &HashSet<PathBuf>, index: &NoteIndex, sender: &Sender<Message>) {
    for path in paths {
        let diagnostics = compute_diagnostics(path, index);
        let params = PublishDiagnosticsParams {
            uri: path_to_uri(path),
            diagnostics,
            version: None,
        };
        let _ = sender.send(Message::Notification(Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::to_value(params).expect("serialize diagnostics"),
        }));
    }
}

// ─── Completion ───────────────────────────────────────────────────────────────

/// Returns `true` if the text on the cursor's line immediately before the
/// cursor position ends with `[[`, indicating the user wants note completion.
fn check_trigger(content: &str, pos: Position) -> bool {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let up_to_cursor = line.get(..pos.character as usize).unwrap_or(line);
    up_to_cursor.ends_with("[[")
}

pub fn handle_completion(params: CompletionParams, index: &NoteIndex) -> Vec<CompletionItem> {
    let pos = params.text_document_position.position;
    let path = uri_to_path(&params.text_document_position.text_document.uri);
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };
    if !check_trigger(&note.content, pos) {
        return vec![];
    }
    index
        .all_notes()
        .map(|n| CompletionItem {
            label: n.stem.clone(),
            kind: Some(CompletionItemKind::FILE),
            ..Default::default()
        })
        .collect()
}

// ─── Document Symbols ─────────────────────────────────────────────────────────

#[allow(deprecated)] // DocumentSymbol.deprecated field
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> DocumentSymbolResponse {
    let path = uri_to_path(&params.text_document.uri);
    let symbols = index
        .get_note(&path)
        .map(|note| {
            note.headings
                .iter()
                .map(|h| DocumentSymbol {
                    name: h.text.clone(),
                    kind: SymbolKind::STRING,
                    range: h.range,
                    selection_range: h.text_range,
                    detail: None,
                    tags: None,
                    deprecated: None,
                    children: None,
                })
                .collect()
        })
        .unwrap_or_default();
    DocumentSymbolResponse::Nested(symbols)
}

// ─── Workspace Symbols ────────────────────────────────────────────────────────

#[allow(deprecated)] // SymbolInformation.deprecated field
pub fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation> {
    let query = params.query.to_lowercase();
    index
        .all_notes()
        .flat_map(|note| {
            note.headings.iter().filter_map(|h| {
                if !query.is_empty() && !h.text.to_lowercase().contains(&query) {
                    return None;
                }
                Some(SymbolInformation {
                    name: h.text.clone(),
                    kind: SymbolKind::STRING,
                    location: Location { uri: path_to_uri(&note.path), range: h.range },
                    container_name: Some(note.stem.clone()),
                    tags: None,
                    deprecated: None,
                })
            })
        })
        .collect()
}

// ─── Go to Definition ─────────────────────────────────────────────────────────

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

fn find_link_at_position(note: &crate::parser::Note, pos: Position) -> Option<&crate::parser::WikiLink> {
    note.wiki_links.iter().find(|link| contains(link.range, pos))
}

pub fn handle_definition(params: GotoDefinitionParams, index: &NoteIndex) -> Option<Location> {
    let pos = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri);
    let note = index.get_note(&path)?;
    let link = find_link_at_position(note, pos)?;
    let ResolvedLink::Found(target_path) = index.resolve(&link.stem) else {
        return None;
    };

    // If the link carries an anchor, navigate to the matching heading.
    // Returns Some(range) when both the target note and a matching heading exist.
    let anchor_range = link.anchor.as_ref().and_then(|anchor| {
        let target_note = index.get_note(&target_path)?;
        let heading = target_note
            .headings
            .iter()
            .find(|h| h.text.to_lowercase() == anchor.to_lowercase())?;
        Some(heading.range)
    });
    if let Some(range) = anchor_range {
        return Some(Location { uri: path_to_uri(&target_path), range });
    }
    // No anchor, or anchor not found → fall through to file top.

    Some(Location {
        uri: path_to_uri(&target_path),
        range: Range::default(),
    })
}

// ─── Find References ──────────────────────────────────────────────────────────

pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location> {
    let pos = params.text_document_position.position;
    let path = uri_to_path(&params.text_document_position.text_document.uri);
    let Some(note) = index.get_note(&path) else { return vec![] };
    let Some(link) = find_link_at_position(note, pos) else { return vec![] };
    let ResolvedLink::Found(target_path) = index.resolve(&link.stem) else { return vec![] };
    index
        .links_to(&target_path)
        .iter()
        .map(|located| Location {
            uri: path_to_uri(&located.source_path),
            range: located.wiki_link.range,
        })
        .collect()
}

// ─── Rename ───────────────────────────────────────────────────────────────────

/// Returns a `WorkspaceEdit` that rewrites every `[[old-stem]]` backlink to
/// use the new stem. The editor applies this edit before performing the rename.
// lsp_types::Uri contains a Cell internally; clippy flags it as a mutable key
// type, but it's the exact type WorkspaceEdit::changes requires.
#[allow(clippy::mutable_key_type)]
pub fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit {
    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    for rename in params.files {
        let old_path = uri_to_path(
            &rename.old_uri.parse().expect("willRenameFiles: invalid old_uri"),
        );
        let new_path = url::Url::parse(&rename.new_uri)
            .expect("willRenameFiles: invalid new_uri")
            .to_file_path()
            .expect("willRenameFiles: new_uri is not a file URI");
        let new_stem = new_path
            .file_stem()
            .expect("willRenameFiles: new_uri has no filename")
            .to_string_lossy()
            .into_owned();

        for located in index.links_to(&old_path) {
            let edit = TextEdit {
                range: located.wiki_link.inner_range,
                new_text: new_stem.clone(),
            };
            changes
                .entry(path_to_uri(&located.source_path))
                .or_default()
                .push(edit);
        }
    }

    WorkspaceEdit { changes: Some(changes), ..Default::default() }
}

// ─── URI utilities ────────────────────────────────────────────────────────────

/// Convert an LSP URI to an absolute filesystem path.
///
/// Panics if the URI is not a `file://` URI (non-file URIs should never reach
/// these handlers in a local Markdown LSP server).
pub fn uri_to_path(uri: &lsp_types::Uri) -> PathBuf {
    url::Url::parse(uri.as_str())
        .expect("invalid URI")
        .to_file_path()
        .expect("non-file URI")
}

/// Convert an absolute filesystem path to an LSP URI.
///
/// Panics if `path` is not absolute.
pub fn path_to_uri(path: &Path) -> lsp_types::Uri {
    url::Url::from_file_path(path)
        .expect("non-absolute path")
        .as_str()
        .parse()
        .expect("file URL should parse as Uri")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{FileRename, RenameFilesParams};

    use super::*;
    use crate::index::NoteIndex;
    use crate::parser;

    fn note(path: &str, content: &str) -> parser::Note {
        parser::parse(Path::new(path), content)
    }

    fn file_uri(path: &str) -> lsp_types::Uri {
        path_to_uri(Path::new(path))
    }

    /// File with two backlinks → WorkspaceEdit with two TextEdits.
    #[test]
    fn rename_produces_edits() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", ""));
        idx.index(note("/vault/a.md", "[[b]]\n[[b]]"));

        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: "file:///vault/b.md".to_string(),
                new_uri: "file:///vault/new-b.md".to_string(),
            }],
        };
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.expect("expected changes");
        let edits = changes.get(&file_uri("/vault/a.md")).expect("expected edits for a.md");
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "new-b"));
    }

    /// File with no backlinks → empty WorkspaceEdit.
    #[test]
    fn rename_no_backlinks_empty_edit() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/lonely.md", ""));

        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: "file:///vault/lonely.md".to_string(),
                new_uri: "file:///vault/new-lonely.md".to_string(),
            }],
        };
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.expect("expected changes map");
        assert!(changes.is_empty(), "expected no changes for a file with no backlinks");
    }

    /// `[[old|alias]]` → edit replaces only the stem; alias is untouched.
    #[test]
    fn rename_preserves_alias() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/old.md", ""));
        idx.index(note("/vault/src.md", "[[old|my alias]]"));

        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: "file:///vault/old.md".to_string(),
                new_uri: "file:///vault/new.md".to_string(),
            }],
        };
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.expect("expected changes");
        let edits = changes.get(&file_uri("/vault/src.md")).expect("expected edits for src.md");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "new");
        // inner_range covers only "old" (chars 2–5 on line 0), not the alias
        assert_eq!(edits[0].range.start.character, 2);
        assert_eq!(edits[0].range.end.character, 5);
    }

    // ── Go to Definition — anchor navigation ─────────────────────────────────

    fn make_definition_params(path: &str, line: u32, character: u32) -> GotoDefinitionParams {
        GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    /// `[[b#Section]]` with b.md having `## Section` → Location on heading line.
    #[test]
    fn definition_anchor_navigates_to_heading() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Section\n"));
        idx.index(note("/vault/a.md", "[[b#Section]]\n"));

        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = handle_definition(params, &idx).expect("expected a location");
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_eq!(loc.range.start.line, 0, "expected to navigate to heading line");
        assert_ne!(loc.range, Range::default(), "expected heading range, not file top");
    }

    /// `[[b#Missing]]` with no matching heading → Location at file top (line 0, col 0).
    #[test]
    fn definition_anchor_not_found_falls_back() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Section\n"));
        idx.index(note("/vault/a.md", "[[b#Missing]]\n"));

        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = handle_definition(params, &idx).expect("expected a location");
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_eq!(loc.range, Range::default(), "expected file top on anchor miss");
    }

    /// `[[b]]` (no anchor) → Location at file top, same as before.
    #[test]
    fn definition_no_anchor_unchanged() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Section\n"));
        idx.index(note("/vault/a.md", "[[b]]\n"));

        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = handle_definition(params, &idx).expect("expected a location");
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_eq!(loc.range, Range::default(), "expected file top for plain link");
    }

    // ── Document Symbols ─────────────────────────────────────────────────────

    /// Note with 3 headings → 3 DocumentSymbols with correct text and level kind.
    #[test]
    fn document_symbols_returns_headings() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "# Title\n\n## Section\n\n### Sub\n"));

        let params = DocumentSymbolParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri("/vault/a.md") },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let DocumentSymbolResponse::Nested(symbols) = handle_document_symbols(params, &idx)
        else {
            panic!("expected Nested response");
        };
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "Title");
        assert_eq!(symbols[1].name, "Section");
        assert_eq!(symbols[2].name, "Sub");
    }

    /// Note with no headings → empty symbol list.
    #[test]
    fn document_symbols_empty() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/plain.md", "Just some prose.\n"));

        let params = DocumentSymbolParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri("/vault/plain.md") },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let DocumentSymbolResponse::Nested(symbols) = handle_document_symbols(params, &idx)
        else {
            panic!("expected Nested response");
        };
        assert!(symbols.is_empty(), "expected no symbols for a file with no headings");
    }

    // ── Workspace Symbols ────────────────────────────────────────────────────

    /// Query "sec" matches only headings containing "sec" (case-insensitive).
    #[test]
    fn workspace_symbols_filtered() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "# Title\n\n## Section\n"));
        idx.index(note("/vault/b.md", "## Other\n"));

        let params = WorkspaceSymbolParams {
            query: "sec".to_string(),
            ..Default::default()
        };
        let symbols = handle_workspace_symbols(params, &idx);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Section");
    }

    /// Empty query returns all headings across all indexed notes.
    #[test]
    fn workspace_symbols_empty_query() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "# Alpha\n\n## Beta\n"));
        idx.index(note("/vault/b.md", "# Gamma\n"));

        let params = WorkspaceSymbolParams { query: String::new(), ..Default::default() };
        let symbols = handle_workspace_symbols(params, &idx);
        assert_eq!(symbols.len(), 3);
    }

    // ── Anchor diagnostics ───────────────────────────────────────────────────

    /// `[[b#Missing]]` with no matching heading in b.md → Warning diagnostic.
    #[test]
    fn anchor_diagnostic_missing() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Exists\n"));
        idx.index(note("/vault/a.md", "[[b#Missing]]\n"));

        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diags[0].message, "Heading not found: '#Missing' in '[[b#Missing]]'");
    }

    /// `[[b#Exists]]` with a matching heading → no diagnostic.
    #[test]
    fn anchor_diagnostic_present() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Exists\n"));
        idx.index(note("/vault/a.md", "[[b#Exists]]\n"));

        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert!(diags.is_empty(), "expected no diagnostic when heading exists");
    }

    /// `[[b#my section]]` matches `## My Section` case-insensitively → no diagnostic.
    #[test]
    fn anchor_diagnostic_case_insensitive() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## My Section\n"));
        idx.index(note("/vault/a.md", "[[b#my section]]\n"));

        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert!(diags.is_empty(), "expected no diagnostic for case-insensitive match");
    }

    // ── File rename ───────────────────────────────────────────────────────────

    /// Two files renamed in one batch → edits produced for both.
    #[test]
    fn rename_multiple_files_in_one_batch() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/x.md", ""));
        idx.index(note("/vault/y.md", ""));
        idx.index(note("/vault/linker.md", "[[x]] and [[y]]"));

        let params = RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: "file:///vault/x.md".to_string(),
                    new_uri: "file:///vault/new-x.md".to_string(),
                },
                FileRename {
                    old_uri: "file:///vault/y.md".to_string(),
                    new_uri: "file:///vault/new-y.md".to_string(),
                },
            ],
        };
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.expect("expected changes");
        let edits =
            changes.get(&file_uri("/vault/linker.md")).expect("expected edits for linker.md");
        assert_eq!(edits.len(), 2);
        let texts: Vec<&str> = edits.iter().map(|e| e.new_text.as_str()).collect();
        assert!(texts.contains(&"new-x"), "expected new-x in edits");
        assert!(texts.contains(&"new-y"), "expected new-y in edits");
    }
}
