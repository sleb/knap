use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crossbeam_channel::Sender;
use log::warn;
use lsp_server::{Message, Notification};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Diagnostic, DiagnosticSeverity,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Location, Position, PublishDiagnosticsParams, Range, ReferenceParams, RenameFilesParams,
    SymbolInformation, SymbolKind, TextEdit, WorkspaceEdit, WorkspaceSymbolParams,
};

use crate::index::{NoteIndex, ResolvedLink};

// ─── Diagnostics ──────────────────────────────────────────────────────────────

pub fn compute_diagnostics(path: &Path, index: &NoteIndex) -> Vec<Diagnostic> {
    let Some(note) = index.get_note(path) else {
        return vec![];
    };

    let mut diagnostics = Vec::new();

    for link in &note.md_links {
        if link.target.is_empty() {
            continue; // anchor-only links; nothing to resolve
        }
        match index.resolve(path, &link.target) {
            ResolvedLink::Broken => {
                diagnostics.push(Diagnostic {
                    range: link.target_range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Link target not found: '{}'", link.target),
                    source: Some("knap".to_string()),
                    ..Default::default()
                });
            }
            ResolvedLink::Found(target_path) => {
                if let Some(anchor) = &link.anchor {
                    let found = index
                        .get_note(&target_path)
                        .map(|n| {
                            n.headings
                                .iter()
                                .any(|h| h.text.to_lowercase() == anchor.to_lowercase())
                        })
                        .unwrap_or(false);
                    if !found {
                        let range = link.anchor_range.unwrap_or(link.range);
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!("Heading not found: '#{anchor}'"),
                            source: Some("knap".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    diagnostics
}

pub fn publish_diagnostics(paths: &HashSet<PathBuf>, index: &NoteIndex, sender: &Sender<Message>) {
    for path in paths {
        let diagnostics = compute_diagnostics(path, index);
        let params = PublishDiagnosticsParams {
            uri: path_to_uri(path),
            diagnostics,
            version: None,
        };
        if let Err(e) = sender.send(Message::Notification(Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::to_value(params).expect("serialize diagnostics"),
        })) {
            warn!("failed to publish diagnostics for {}: {e}", path.display());
        }
    }
}

// ─── Completion ───────────────────────────────────────────────────────────────

/// Convert a UTF-16 code unit offset (LSP `Position.character`) to a UTF-8
/// byte offset within `s`. Clamps to `s.len()` when the offset exceeds the
/// line length.
fn utf16_to_byte_offset(s: &str, utf16_offset: u32) -> usize {
    let mut byte = 0;
    let mut utf16 = 0u32;
    for ch in s.chars() {
        if utf16 >= utf16_offset {
            break;
        }
        utf16 += ch.len_utf16() as u32;
        byte += ch.len_utf8();
    }
    byte
}

/// Returns `true` if the text on the cursor's line immediately before the
/// cursor ends with `](`, indicating the user is about to type a link path.
fn check_link_trigger(content: &str, pos: Position) -> bool {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    line[..cursor].ends_with("](")
}

/// Compute the relative path from `from_dir` to `to`, suitable as a Markdown
/// link target. Both arguments must be absolute paths.
fn relative_path(from_dir: &Path, to: &Path) -> String {
    let from: Vec<Component> = from_dir.components().collect();
    let to_comps: Vec<Component> = to.components().collect();

    let common = from.iter().zip(to_comps.iter()).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    for _ in 0..(from.len() - common) {
        result.push("..");
    }
    for c in &to_comps[common..] {
        result.push(c.as_os_str());
    }
    result.to_string_lossy().into_owned()
}

pub fn handle_completion(params: CompletionParams, index: &NoteIndex) -> Vec<CompletionItem> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };
    if !check_link_trigger(&note.content, pos) {
        return vec![];
    }
    let from_dir = path.parent().unwrap_or(Path::new(""));
    let notes = index.all_notes().filter(|n| n.path != path).map(|n| {
        let rel = relative_path(from_dir, &n.path);
        let title = n
            .frontmatter
            .as_ref()
            .and_then(|fm| fm.title.as_deref())
            .map(str::to_owned);
        let label = title.clone().unwrap_or_else(|| rel.clone());
        let detail = title.map(|_| rel.clone());
        CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            filter_text: Some(rel.clone()),
            insert_text: Some(rel),
            detail,
            ..Default::default()
        }
    });
    let attachments = index.all_attachment_paths().map(|p| {
        let rel = relative_path(from_dir, p);
        let label = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| rel.clone());
        CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            filter_text: Some(rel.clone()),
            insert_text: Some(rel),
            ..Default::default()
        }
    });
    notes.chain(attachments).collect()
}

// ─── Go to Definition ─────────────────────────────────────────────────────────

fn contains(range: Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

fn find_md_link_at_position(
    note: &crate::parser::Note,
    pos: Position,
) -> Option<&crate::parser::MarkdownLink> {
    note.md_links.iter().find(|link| contains(link.range, pos))
}

pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse> {
    let pos = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let note = index.get_note(&path)?;

    let link = find_md_link_at_position(note, pos)?;
    let ResolvedLink::Found(target_path) = index.resolve(&path, &link.target) else {
        return None;
    };
    let anchor_range = link.anchor.as_ref().and_then(|anchor| {
        let target_note = index.get_note(&target_path)?;
        let heading = target_note
            .headings
            .iter()
            .find(|h| h.text.to_lowercase() == anchor.to_lowercase())?;
        Some(heading.range)
    });
    let range = anchor_range.unwrap_or_default();
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: path_to_uri(&target_path),
        range,
    }))
}

// ─── Find References ──────────────────────────────────────────────────────────

pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };

    let target_path = if let Some(link) = find_md_link_at_position(note, pos) {
        match index.resolve(&path, &link.target) {
            ResolvedLink::Found(p) => p,
            ResolvedLink::Broken => return vec![],
        }
    } else {
        path.clone()
    };

    index
        .links_to(&target_path)
        .iter()
        .map(|located| Location {
            uri: path_to_uri(&located.source_path),
            range: located.md_link.range,
        })
        .collect()
}

// ─── Rename ───────────────────────────────────────────────────────────────────

#[allow(clippy::mutable_key_type)] // lsp_types::Uri has interior mutability; HashMap<Uri, _> is the LSP-spec type
pub fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit {
    use crate::index::{looks_like_url, normalize_path};

    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    for file_rename in params.files {
        let Some(old_path) = url::Url::parse(&file_rename.old_uri)
            .ok()
            .and_then(|u| u.to_file_path().ok())
        else {
            continue;
        };
        let Some(new_path) = url::Url::parse(&file_rename.new_uri)
            .ok()
            .and_then(|u| u.to_file_path().ok())
        else {
            continue;
        };
        let old_dir = old_path.parent().unwrap_or(Path::new(""));
        let new_dir = new_path.parent().unwrap_or(Path::new(""));

        // Incoming: other notes linking to old_path need their target updated.
        for located in index.links_to(&old_path) {
            let source_dir = located.source_path.parent().unwrap_or(Path::new(""));
            let new_target = relative_path(source_dir, &new_path);
            changes
                .entry(path_to_uri(&located.source_path))
                .or_default()
                .push(TextEdit { range: located.md_link.target_range, new_text: new_target });
        }

        // Outgoing: links inside the renamed file that point to other files may
        // need updating if the file moves to a different directory.
        if let Some(note) = index.get_note(&old_path) {
            for link in &note.md_links {
                if link.target.is_empty() || looks_like_url(&link.target) {
                    continue;
                }
                let abs_target = normalize_path(&old_dir.join(&link.target));
                let new_target = relative_path(new_dir, &abs_target);
                if new_target != link.target {
                    changes
                        .entry(path_to_uri(&old_path))
                        .or_default()
                        .push(TextEdit { range: link.target_range, new_text: new_target });
                }
            }
        }
    }

    WorkspaceEdit { changes: Some(changes), ..Default::default() }
}

// ─── Document Symbols ─────────────────────────────────────────────────────────

#[allow(deprecated)] // SymbolInformation::deprecated field is itself deprecated in lsp-types
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> Option<DocumentSymbolResponse> {
    let path = uri_to_path(&params.text_document.uri)?;
    let note = index.get_note(&path)?;
    let symbols = note
        .headings
        .iter()
        .map(|h| SymbolInformation {
            name: h.text.clone(),
            kind: SymbolKind::STRING,
            location: Location { uri: path_to_uri(&path), range: h.range },
            tags: None,
            deprecated: None,
            container_name: None,
        })
        .collect();
    Some(DocumentSymbolResponse::Flat(symbols))
}

// ─── Workspace Symbols ────────────────────────────────────────────────────────

#[allow(deprecated)]
pub fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation> {
    let query = params.query.to_lowercase();
    index
        .all_notes()
        .flat_map(|note| {
            note.headings.iter().filter_map(|h| {
                if query.is_empty() || h.text.to_lowercase().contains(&query) {
                    Some(SymbolInformation {
                        name: h.text.clone(),
                        kind: SymbolKind::STRING,
                        location: Location { uri: path_to_uri(&note.path), range: h.range },
                        container_name: Some(
                            note.path.file_name().unwrap_or_default().to_string_lossy().into_owned()
                        ),
                        tags: None,
                        deprecated: None,
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

// ─── URI utilities ────────────────────────────────────────────────────────────

/// Convert an LSP URI to an absolute filesystem path.
///
/// Returns `None` for non-`file://` URIs (e.g. `untitled:` or
/// `vscode-notebook-cell:`). Callers should silently skip `None` — there is
/// nothing useful to index or serve for a buffer without a path.
pub fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
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

    use lsp_types::{
        CompletionParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
        Position, ReferenceParams, SymbolKind, WorkspaceSymbolParams,
    };

    use super::*;
    use crate::index::NoteIndex;
    use crate::parser;

    fn note(path: &str, content: &str) -> parser::Note {
        parser::parse(Path::new(path), content)
    }

    fn file_uri(path: &str) -> lsp_types::Uri {
        path_to_uri(Path::new(path))
    }

    fn make_completion_params(path: &str, line: u32, character: u32) -> CompletionParams {
        CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        }
    }

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

    fn make_references_params(path: &str, line: u32, character: u32) -> ReferenceParams {
        ReferenceParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp_types::ReferenceContext { include_declaration: false },
        }
    }

    fn unwrap_scalar(resp: Option<GotoDefinitionResponse>) -> Location {
        match resp.expect("expected a response") {
            GotoDefinitionResponse::Scalar(loc) => loc,
            other => panic!("expected Scalar, got {:?}", other),
        }
    }

    // ── relative_path ─────────────────────────────────────────────────────────

    #[test]
    fn relative_path_same_dir() {
        let from = Path::new("/vault");
        let to = Path::new("/vault/b.md");
        assert_eq!(relative_path(from, to), "b.md");
    }

    #[test]
    fn relative_path_parent_dir() {
        let from = Path::new("/vault/sub");
        let to = Path::new("/vault/b.md");
        assert_eq!(relative_path(from, to), "../b.md");
    }

    #[test]
    fn relative_path_subdirectory() {
        let from = Path::new("/vault");
        let to = Path::new("/vault/sub/c.md");
        assert_eq!(relative_path(from, to), "sub/c.md");
    }

    // ── compute_diagnostics ───────────────────────────────────────────────────

    #[test]
    fn diagnostics_broken_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](missing.md)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing.md"));
    }

    #[test]
    fn diagnostics_valid_link_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[text](b.md)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_broken_anchor() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Existing\n"));
        idx.seed(note("/vault/a.md", "[text](b.md#Missing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing"));
    }

    #[test]
    fn diagnostics_valid_anchor_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Existing\n"));
        idx.seed(note("/vault/a.md", "[text](b.md#Existing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_anchor_only_skipped() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](#section)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx);
        assert!(diags.is_empty(), "anchor-only links should not produce diagnostics");
    }

    // ── handle_completion ─────────────────────────────────────────────────────

    #[test]
    fn completion_no_trigger_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "hello world"));
        let params = make_completion_params("/vault/a.md", 0, 5);
        let items = handle_completion(params, &idx);
        assert!(items.is_empty());
    }

    #[test]
    fn completion_trigger_returns_notes() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        // "[link](" → cursor at position 7 (after the `(`)
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx);
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.insert_text.as_deref() == Some("b.md")));
    }

    #[test]
    fn completion_relative_path_subdirectory() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/sub/b.md", ""));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx);
        assert!(items.iter().any(|i| i.insert_text.as_deref() == Some("sub/b.md")));
    }

    #[test]
    fn completion_title_used_as_label() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx);
        let item = items.iter().find(|i| i.insert_text.as_deref() == Some("b.md")).unwrap();
        assert_eq!(item.label, "My Note");
        assert_eq!(item.detail.as_deref(), Some("b.md"));
    }

    #[test]
    fn completion_includes_attachments() {
        let mut idx = NoteIndex::default();
        let _ = idx.add_attachment(std::path::PathBuf::from("/vault/img.png"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx);
        assert!(items.iter().any(|i| i.insert_text.as_deref() == Some("img.png")));
    }

    #[test]
    fn completion_attachment_label_is_filename() {
        let mut idx = NoteIndex::default();
        let _ = idx.add_attachment(std::path::PathBuf::from("/vault/sub/report.pdf"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx);
        let item = items
            .iter()
            .find(|i| i.insert_text.as_deref() == Some("sub/report.pdf"))
            .unwrap();
        assert_eq!(item.label, "report.pdf");
        assert_eq!(item.filter_text.as_deref(), Some("sub/report.pdf"));
    }

    // ── handle_definition ─────────────────────────────────────────────────────

    #[test]
    fn definition_navigates_to_file_top() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = unwrap_scalar(handle_definition(params, &idx));
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_eq!(loc.range, Range::default());
    }

    #[test]
    fn definition_navigates_to_heading() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Section\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#Section)"));
        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = unwrap_scalar(handle_definition(params, &idx));
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_ne!(loc.range, Range::default());
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn definition_missing_anchor_falls_back_to_top() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Section\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#Missing)"));
        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = unwrap_scalar(handle_definition(params, &idx));
        assert!(loc.uri.as_str().ends_with("b.md"));
        assert_eq!(loc.range, Range::default());
    }

    #[test]
    fn definition_broken_link_returns_none() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let params = make_definition_params("/vault/a.md", 0, 3);
        assert!(handle_definition(params, &idx).is_none());
    }

    // ── handle_references ─────────────────────────────────────────────────────

    #[test]
    fn references_from_link_returns_backlinks() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        // cursor on `[link](b.md)` in a.md → backlinks to b.md
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("a.md"));
    }

    #[test]
    fn references_fallback_returns_backlinks_to_self() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "[link](a.md)"));
        idx.seed(note("/vault/a.md", "no links here"));
        // cursor at (0, 0) in a.md — no link, so fallback to links_to(a.md)
        let params = make_references_params("/vault/a.md", 0, 0);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("b.md"));
    }

    #[test]
    fn references_broken_link_returns_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert!(locs.is_empty());
    }

    // ── handle_will_rename_files ──────────────────────────────────────────────

    fn make_rename_params(old_path: &str, new_path: &str) -> lsp_types::RenameFilesParams {
        lsp_types::RenameFilesParams {
            files: vec![lsp_types::FileRename {
                old_uri: format!("file://{old_path}"),
                new_uri: format!("file://{new_path}"),
            }],
        }
    }

    #[test]
    fn rename_updates_incoming_links() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", ""));
        idx.seed(note("/vault/b.md", "[link](a.md)"));
        let params = make_rename_params("/vault/a.md", "/vault/sub/a.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap();
        let b_uri = file_uri("/vault/b.md");
        assert!(changes.contains_key(&b_uri), "b.md should have edits");
        assert_eq!(changes[&b_uri].len(), 1);
        assert_eq!(changes[&b_uri][0].new_text, "sub/a.md");
    }

    #[test]
    fn rename_updates_outgoing_links() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/sub/a.md", "[link](../b.md)"));
        let params = make_rename_params("/vault/sub/a.md", "/vault/a.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap();
        let a_uri = file_uri("/vault/sub/a.md");
        assert!(changes.contains_key(&a_uri), "a.md should have outgoing edits");
        assert_eq!(changes[&a_uri].len(), 1);
        assert_eq!(changes[&a_uri][0].new_text, "b.md");
    }

    #[test]
    fn rename_updates_both_incoming_and_outgoing() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/c.md", ""));
        idx.seed(note("/vault/a.md", "[link](c.md)"));
        idx.seed(note("/vault/b.md", "[link](a.md)"));
        let params = make_rename_params("/vault/a.md", "/vault/sub/a.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap();
        let b_uri = file_uri("/vault/b.md");
        let a_uri = file_uri("/vault/a.md");
        assert!(changes.contains_key(&b_uri), "b.md should have incoming edit");
        assert_eq!(changes[&b_uri][0].new_text, "sub/a.md");
        assert!(changes.contains_key(&a_uri), "a.md should have outgoing edit");
        assert_eq!(changes[&a_uri][0].new_text, "../c.md");
    }

    #[test]
    fn rename_skips_url_targets() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[ext](https://example.com)"));
        let params = make_rename_params("/vault/a.md", "/vault/sub/a.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap_or_default();
        assert!(changes.is_empty());
    }

    #[test]
    fn rename_no_changes_same_dir() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        // rename a.md → a2.md within same directory; outgoing link "b.md" unchanged
        let params = make_rename_params("/vault/a.md", "/vault/a2.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap_or_default();
        assert!(changes.is_empty());
    }

    #[test]
    fn rename_unlinked_file_empty_edit() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/c.md", ""));
        let params = make_rename_params("/vault/c.md", "/vault/d.md");
        let edit = handle_will_rename_files(params, &idx);
        let changes = edit.changes.unwrap_or_default();
        assert!(changes.is_empty());
    }

    // ── handle_document_symbols ───────────────────────────────────────────────

    fn make_document_symbol_params(path: &str) -> DocumentSymbolParams {
        DocumentSymbolParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    fn unwrap_flat(resp: Option<DocumentSymbolResponse>) -> Vec<lsp_types::SymbolInformation> {
        match resp.expect("expected Some response") {
            DocumentSymbolResponse::Flat(syms) => syms,
            DocumentSymbolResponse::Nested(_) => panic!("expected Flat, got Nested"),
        }
    }

    #[test]
    fn document_symbols_returns_all_headings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Heading One\n## Heading Two\n### Heading Three\n"));
        let params = make_document_symbol_params("/vault/a.md");
        let syms = unwrap_flat(handle_document_symbols(params, &idx));
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Heading One");
        assert_eq!(syms[1].name, "Heading Two");
        assert_eq!(syms[2].name, "Heading Three");
    }

    #[test]
    fn document_symbols_note_absent_returns_none() {
        let idx = NoteIndex::default();
        let params = make_document_symbol_params("/vault/a.md");
        assert!(handle_document_symbols(params, &idx).is_none());
    }

    #[test]
    fn document_symbols_no_headings_returns_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "just prose, no headings"));
        let params = make_document_symbol_params("/vault/a.md");
        let syms = unwrap_flat(handle_document_symbols(params, &idx));
        assert!(syms.is_empty());
    }

    #[test]
    fn document_symbols_kind_is_string() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# H1\n## H2\n"));
        let params = make_document_symbol_params("/vault/a.md");
        let syms = unwrap_flat(handle_document_symbols(params, &idx));
        assert!(syms.iter().all(|s| s.kind == SymbolKind::STRING));
    }

    #[test]
    fn document_symbols_range_matches_heading() {
        let content = "# My Heading\n";
        let heading_range = note("/vault/a.md", content).headings[0].range;
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", content));
        let params = make_document_symbol_params("/vault/a.md");
        let syms = unwrap_flat(handle_document_symbols(params, &idx));
        assert_eq!(syms[0].location.range, heading_range);
    }

    // ── handle_workspace_symbols ──────────────────────────────────────────────

    fn make_workspace_symbol_params(query: &str) -> WorkspaceSymbolParams {
        WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    #[test]
    fn workspace_symbols_empty_query_returns_all() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Alpha\n## Beta\n"));
        idx.seed(note("/vault/b.md", "# Gamma\n"));
        let params = make_workspace_symbol_params("");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 3);
    }

    #[test]
    fn workspace_symbols_query_filters() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Introduction\n## Details\n"));
        let params = make_workspace_symbol_params("intro");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Introduction");
    }

    #[test]
    fn workspace_symbols_no_match_returns_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Heading\n"));
        let params = make_workspace_symbol_params("zzz");
        let syms = handle_workspace_symbols(params, &idx);
        assert!(syms.is_empty());
    }

    #[test]
    fn workspace_symbols_container_is_filename() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/notes/my-note.md", "# Section\n"));
        let params = make_workspace_symbol_params("section");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].container_name.as_deref(), Some("my-note.md"));
    }

    #[test]
    fn workspace_symbols_multiple_notes() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Introduction\n"));
        idx.seed(note("/vault/b.md", "# Introduction\n"));
        let params = make_workspace_symbol_params("intro");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 2);
    }
}
