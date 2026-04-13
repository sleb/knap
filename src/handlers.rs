// Steps 6–9: completion, definition, references, and diagnostics.
// See docs/design/components/handlers.md

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use lsp_server::{Message, Notification};
use lsp_types::{Diagnostic, DiagnosticSeverity, PublishDiagnosticsParams};

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
                message: format!("No note found for '[[{}]]'", link.stem),
                source: Some("knap".to_string()),
                ..Default::default()
            }),
            ResolvedLink::Ambiguous(paths) => Some(Diagnostic {
                range: link.inner_range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!(
                    "'[[{}]]' matches multiple notes: {}",
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
