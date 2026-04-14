// Steps 6–9: completion, definition, references, and diagnostics.
// See docs/design/components/handlers.md

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use lsp_server::{Message, Notification};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Diagnostic, DiagnosticSeverity,
    GotoDefinitionParams, Location, Position, PublishDiagnosticsParams, Range, ReferenceParams,
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
            ResolvedLink::Found(_) => None,
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
    match index.resolve(&link.stem) {
        ResolvedLink::Found(target_path) => Some(Location {
            uri: path_to_uri(&target_path),
            range: Range::default(),
        }),
        _ => None,
    }
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
