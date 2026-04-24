// Steps 6–9: completion, definition, references, and diagnostics.
// See docs/design/components/handlers.md

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use lsp_server::{Message, Notification};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Diagnostic, DiagnosticSeverity,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, Location, MarkupContent,
    MarkupKind, Position, PrepareRenameResponse, PublishDiagnosticsParams, Range, ReferenceParams,
    RenameFilesParams, RenameParams, SymbolInformation, SymbolKind, TextDocumentPositionParams,
    TextEdit, WorkspaceEdit, WorkspaceSymbolParams,
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
/// cursor position ends with `[[`, indicating the user wants note completion.
fn check_trigger(content: &str, pos: Position) -> bool {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    line[..cursor].ends_with("[[")
}

/// Returns the line number (0-indexed) of the frontmatter closing `---`, or
/// `None` when the content has no valid frontmatter block.
fn frontmatter_close_line(content: &str) -> Option<usize> {
    let offset = crate::parser::frontmatter_body_offset(content);
    if offset == 0 { None } else { Some(content[..offset].lines().count() - 1) }
}

/// Returns `true` when the cursor is inside the frontmatter block in a position
/// that calls for tag completions:
/// 1. The cursor is on the `tags:` line itself (any column), or
/// 2. The cursor is on a `- ` list-item line that follows a bare `tags:` key
///    within the same frontmatter block.
fn check_tag_trigger(content: &str, pos: Position) -> bool {
    let close_line = match frontmatter_close_line(content) {
        Some(l) => l,
        None => return false,
    };
    // Cursor must be inside the frontmatter (after opening `---`, before closing `---`)
    if pos.line == 0 || pos.line as usize >= close_line {
        return false;
    }

    let lines: Vec<&str> = content.lines().collect();
    let current = lines.get(pos.line as usize).unwrap_or(&"");

    // Pattern 1: cursor is on the `tags:` line
    if current.trim_start().starts_with("tags:") {
        return true;
    }

    // Pattern 2: cursor is on a `- ` list item; scan backwards for a bare `tags:` key
    let cursor = utf16_to_byte_offset(current, pos.character);
    let up_to_cursor = &current[..cursor];
    if up_to_cursor.trim_start().starts_with('-') || up_to_cursor.trim() == "-" {
        for i in (1..pos.line as usize).rev() {
            let prev = lines[i].trim();
            if prev == "tags:" {
                return true;
            }
            // Any non-empty, non-list line that contains `:` is a different YAML key — stop
            if !prev.is_empty() && !prev.starts_with('-') && prev.contains(':') {
                break;
            }
        }
    }

    false
}

fn tag_completions(index: &NoteIndex) -> Vec<CompletionItem> {
    index
        .all_tags()
        .map(|tag| CompletionItem {
            label: tag.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            insert_text: Some(tag.to_string()),
            ..Default::default()
        })
        .collect()
}

pub fn handle_completion(params: CompletionParams, index: &NoteIndex) -> Vec<CompletionItem> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };
    if check_tag_trigger(&note.content, pos) {
        return tag_completions(index);
    }
    if !check_trigger(&note.content, pos) {
        return vec![];
    }
    index
        .all_notes()
        .map(|n| {
            let title = n
                .frontmatter
                .as_ref()
                .and_then(|fm| fm.title.as_deref())
                .map(str::to_owned);
            let label = title.clone().unwrap_or_else(|| n.stem.clone());
            let detail = if title.is_some() { Some(n.stem.clone()) } else { None };
            CompletionItem {
                label,
                kind: Some(CompletionItemKind::FILE),
                filter_text: Some(n.stem.clone()),
                insert_text: Some(n.stem.clone()),
                detail,
                ..Default::default()
            }
        })
        .collect()
}

// ─── Hover ────────────────────────────────────────────────────────────────────

const PREVIEW_LINES: usize = 10;

/// Returns the body of `content` with the YAML frontmatter block stripped.
/// Delegates to `frontmatter_body_offset` so the logic stays in one place.
fn body_after_frontmatter(content: &str) -> &str {
    &content[crate::parser::frontmatter_body_offset(content)..]
}

/// Build a Markdown hover-preview string: `**title**\n\n<body>` where body is
/// the first `PREVIEW_LINES` lines after any frontmatter, followed by `\n…`
/// when truncated.
pub fn render_preview(note: &crate::parser::Note) -> String {
    let title = note
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.title.as_deref())
        .unwrap_or(&note.stem);

    let body = body_after_frontmatter(&note.content);
    let lines: Vec<&str> = body.lines().collect();

    let (preview, truncated) = if lines.len() <= PREVIEW_LINES {
        (lines.join("\n"), false)
    } else {
        (lines[..PREVIEW_LINES].join("\n"), true)
    };

    let suffix = if truncated { "\n\u{2026}" } else { "" };
    format!("**{title}**\n\n{preview}{suffix}")
}

/// Returns `true` for targets that are external URLs (http, https, //, mailto,
/// ftp). Local relative paths return `false`.
fn is_external_url(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("//")
        || target.starts_with("mailto:")
        || target.starts_with("ftp://")
}

/// Normalize `.` and `..` components in `path` without touching the filesystem.
fn normalize_path(path: &std::path::Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}                    // drop `.`
            Component::ParentDir => { out.pop(); }     // resolve `..`
            c => out.push(c),
        }
    }
    out.iter().collect()
}

fn find_md_link_at_position(
    note: &crate::parser::Note,
    pos: Position,
) -> Option<&crate::parser::MarkdownLink> {
    note.md_links.iter().find(|link| contains(link.range, pos))
}

pub fn handle_hover(params: HoverParams, index: &NoteIndex) -> Option<Hover> {
    let pos = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let note = index.get_note(&path)?;

    // 1. Wiki-link at cursor position.
    if let Some(link) = find_link_at_position(note, pos) {
        let ResolvedLink::Found(target_path) = index.resolve(&link.stem) else {
            return None; // broken or ambiguous — diagnostic already covers this
        };
        let target = index.get_note(&target_path)?;
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: render_preview(target),
            }),
            range: Some(link.range),
        });
    }

    // 2. Standard Markdown link or image at cursor position.
    if let Some(md_link) = find_md_link_at_position(note, pos) {
        let value = if md_link.is_image {
            format!("**Image**\n\n`{}`", md_link.target)
        } else if is_external_url(&md_link.target) {
            format!("[{}]({})", md_link.text, md_link.target)
        } else {
            // Local path: resolve relative to the current file's directory.
            let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
            let resolved = normalize_path(&parent.join(&md_link.target));
            if let Some(target_note) = index.get_note(&resolved) {
                render_preview(target_note)
            } else {
                format!("`{}`", md_link.target)
            }
        };
        let range = md_link.range;
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(range),
        });
    }

    None
}

// ─── Document Symbols ─────────────────────────────────────────────────────────

#[allow(deprecated)] // DocumentSymbol.deprecated field
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> DocumentSymbolResponse {
    let symbols = uri_to_path(&params.text_document.uri)
        .as_ref()
        .and_then(|p| index.get_note(p))
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

fn find_tag_at_position(note: &crate::parser::Note, pos: Position) -> Option<&crate::parser::Tag> {
    note.frontmatter
        .as_ref()?
        .tags
        .iter()
        .find(|t| contains(t.range, pos))
}

fn locations_for_tag(tag: &str, index: &NoteIndex) -> Vec<Location> {
    index
        .notes_by_tag(tag)
        .iter()
        .filter_map(|note| {
            let tag_range = note
                .frontmatter
                .as_ref()?
                .tags
                .iter()
                .find(|t| t.name.to_lowercase() == tag.to_lowercase())?
                .range;
            Some(Location { uri: path_to_uri(&note.path), range: tag_range })
        })
        .collect()
}

pub fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse> {
    let pos = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let note = index.get_note(&path)?;

    // 1. Wiki-link at cursor position.
    if let Some(link) = find_link_at_position(note, pos) {
        let ResolvedLink::Found(target_path) = index.resolve(&link.stem) else {
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
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: path_to_uri(&target_path),
            range,
        }));
    }

    // 2. Tag in frontmatter at cursor position.
    if let Some(tag) = find_tag_at_position(note, pos) {
        let locs = locations_for_tag(&tag.name, index);
        return Some(GotoDefinitionResponse::Array(locs));
    }

    None
}

// ─── Find References ──────────────────────────────────────────────────────────

pub fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else { return vec![] };
    let Some(note) = index.get_note(&path) else { return vec![] };

    // 1. Wiki-link at cursor position.
    if let Some(link) = find_link_at_position(note, pos) {
        let ResolvedLink::Found(target_path) = index.resolve(&link.stem) else { return vec![] };
        return index
            .links_to(&target_path)
            .iter()
            .map(|located| Location {
                uri: path_to_uri(&located.source_path),
                range: located.wiki_link.range,
            })
            .collect();
    }

    // 2. Tag in frontmatter at cursor position.
    if let Some(tag) = find_tag_at_position(note, pos) {
        return locations_for_tag(&tag.name, index);
    }

    vec![]
}

// ─── Heading Rename ───────────────────────────────────────────────────────────

/// Returns `RangeWithPlaceholder` covering the heading text when the cursor is
/// on a heading line; `None` otherwise (editor shows "nothing to rename").
pub fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse> {
    let path = uri_to_path(&params.text_document.uri)?;
    let note = index.get_note(&path)?;
    let heading = note.headings.iter().find(|h| contains(h.range, params.position))?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: heading.text_range,
        placeholder: heading.text.clone(),
    })
}

/// Builds a `WorkspaceEdit` that:
/// 1. Rewrites the heading text in its own file.
/// 2. Rewrites every `[[note#OldText]]` anchor whose stem resolves to the
///    heading's file and whose anchor matches the old text (case-insensitive).
///
/// Returns `None` when the cursor is not on any heading.
#[allow(clippy::mutable_key_type)]
pub fn handle_rename(params: RenameParams, index: &NoteIndex) -> Option<WorkspaceEdit> {
    let path = uri_to_path(&params.text_document_position.text_document.uri)?;
    let pos = params.text_document_position.position;

    // Extract the heading's data in a scoped block so the borrow of `index`
    // via `get_note` is released before the iterator loop below.
    let (old_text, text_range) = {
        let note = index.get_note(&path)?;
        let h = note.headings.iter().find(|h| contains(h.range, pos))?;
        (h.text.clone(), h.text_range)
    };

    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    // 1. Rewrite the heading text itself.
    changes
        .entry(path_to_uri(&path))
        .or_default()
        .push(TextEdit { range: text_range, new_text: params.new_name.clone() });

    // 2. Rewrite every anchor link that resolves to this heading.
    for note in index.all_notes() {
        for link in &note.wiki_links {
            let Some(anchor) = &link.anchor else { continue };
            if anchor.to_lowercase() != old_text.to_lowercase() {
                continue;
            }
            let ResolvedLink::Found(target) = index.resolve(&link.stem) else { continue };
            if target != path {
                continue;
            }
            let Some(anchor_range) = link.anchor_range else { continue };
            changes
                .entry(path_to_uri(&note.path))
                .or_default()
                .push(TextEdit { range: anchor_range, new_text: params.new_name.clone() });
        }
    }

    Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

// ─── File Rename ──────────────────────────────────────────────────────────────

/// Returns a `WorkspaceEdit` that rewrites every `[[old-stem]]` backlink to
/// use the new stem. The editor applies this edit before performing the rename.
// lsp_types::Uri contains a Cell internally; clippy flags it as a mutable key
// type, but it's the exact type WorkspaceEdit::changes requires.
#[allow(clippy::mutable_key_type)]
pub fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit {
    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    for rename in params.files {
        let Some(old_path) = uri_to_path(&rename.old_uri.parse().expect("willRenameFiles: invalid old_uri")) else { continue; };
        let Some(new_path) = uri_to_path(&rename.new_uri.parse().expect("willRenameFiles: invalid new_uri")) else { continue; };
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

    fn unwrap_scalar(resp: Option<GotoDefinitionResponse>) -> Location {
        match resp.expect("expected a response") {
            GotoDefinitionResponse::Scalar(loc) => loc,
            other => panic!("expected Scalar, got {:?}", other),
        }
    }

    /// `[[b#Section]]` with b.md having `## Section` → Location on heading line.
    #[test]
    fn definition_anchor_navigates_to_heading() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "## Section\n"));
        idx.index(note("/vault/a.md", "[[b#Section]]\n"));

        let params = make_definition_params("/vault/a.md", 0, 3);
        let loc = unwrap_scalar(handle_definition(params, &idx));
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
        let loc = unwrap_scalar(handle_definition(params, &idx));
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
        let loc = unwrap_scalar(handle_definition(params, &idx));
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

    // ── Heading rename ───────────────────────────────────────────────────────

    fn make_position_params(path: &str, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            position: Position { line, character },
        }
    }

    fn make_rename_params(path: &str, line: u32, character: u32, new_name: &str) -> RenameParams {
        RenameParams {
            text_document_position: make_position_params(path, line, character),
            new_name: new_name.to_string(),
            work_done_progress_params: Default::default(),
        }
    }

    /// Cursor on heading → WorkspaceEdit contains TextEdit at text_range.
    #[test]
    fn rename_heading_updates_heading_text() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/target.md", "## Old Text\n"));

        let params = make_rename_params("/vault/target.md", 0, 5, "New Text");
        let edit = handle_rename(params, &idx).expect("expected a WorkspaceEdit");
        let changes = edit.changes.expect("expected changes");
        let edits = changes.get(&file_uri("/vault/target.md")).expect("expected edits");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "New Text");
        // text_range covers "Old Text": chars 3–11 on line 0
        assert_eq!(edits[0].range.start.character, 3);
        assert_eq!(edits[0].range.end.character, 11);
    }

    /// Two files with `[[target#Old Text]]` → both anchor_range edits included.
    #[test]
    fn rename_heading_updates_anchor_links() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/target.md", "## Old Text\n"));
        idx.index(note("/vault/s1.md", "[[target#Old Text]]\n"));
        idx.index(note("/vault/s2.md", "[[target#Old Text]]\n"));

        let params = make_rename_params("/vault/target.md", 0, 5, "New Text");
        let edit = handle_rename(params, &idx).expect("expected a WorkspaceEdit");
        let changes = edit.changes.expect("expected changes");
        assert!(changes.contains_key(&file_uri("/vault/s1.md")), "expected edit for s1.md");
        assert!(changes.contains_key(&file_uri("/vault/s2.md")), "expected edit for s2.md");
        assert_eq!(changes[&file_uri("/vault/s1.md")][0].new_text, "New Text");
        assert_eq!(changes[&file_uri("/vault/s2.md")][0].new_text, "New Text");
    }

    /// `[[target#old text]]` (lowercase) matches `## Old Text` → anchor edit included.
    #[test]
    fn rename_heading_case_insensitive_match() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/target.md", "## Old Text\n"));
        idx.index(note("/vault/src.md", "[[target#old text]]\n"));

        let params = make_rename_params("/vault/target.md", 0, 5, "New Text");
        let edit = handle_rename(params, &idx).expect("expected a WorkspaceEdit");
        let changes = edit.changes.expect("expected changes");
        assert!(changes.contains_key(&file_uri("/vault/src.md")), "expected edit for src.md");
    }

    /// Cursor not on any heading → `None`.
    #[test]
    fn rename_heading_no_match_returns_none() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/prose.md", "## Heading\n\nJust prose here.\n"));

        // Cursor on the prose line (line 2), not on the heading
        let params = make_rename_params("/vault/prose.md", 2, 5, "Anything");
        assert!(handle_rename(params, &idx).is_none(), "expected None for cursor off heading");
    }

    /// Cursor on heading → `RangeWithPlaceholder` with text_range and heading text.
    #[test]
    fn prepare_rename_on_heading() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/target.md", "## Old Text\n"));

        let params = make_position_params("/vault/target.md", 0, 5);
        let resp = handle_prepare_rename(params, &idx).expect("expected a response");
        let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = resp else {
            panic!("expected RangeWithPlaceholder");
        };
        assert_eq!(placeholder, "Old Text");
        assert_eq!(range.start.character, 3, "range should start after '## '");
        assert_eq!(range.end.character, 11, "range should end at end of text");
    }

    /// Cursor not on any heading → `None`.
    #[test]
    fn prepare_rename_not_on_heading() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/prose.md", "## Heading\n\nJust prose here.\n"));

        let params = make_position_params("/vault/prose.md", 2, 5);
        assert!(handle_prepare_rename(params, &idx).is_none(), "expected None off heading");
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

    // ── completion ────────────────────────────────────────────────────────────

    /// Note with a frontmatter title → label is the title; insert_text and
    /// filter_text are the stem; detail disambiguates with the stem.
    #[test]
    fn completion_uses_title_as_label() {
        use lsp_types::TextDocumentIdentifier;

        let mut idx = NoteIndex::default();
        idx.index(note("/vault/titled.md", "---\ntitle: My Title\n---\nBody.\n"));
        idx.index(note("/vault/cursor.md", "[["));

        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: file_uri("/vault/cursor.md") },
                position: Position { line: 0, character: 2 },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let items = handle_completion(params, &idx);
        let item = items
            .iter()
            .find(|i| i.filter_text.as_deref() == Some("titled"))
            .expect("item for titled.md not found");
        assert_eq!(item.label, "My Title");
        assert_eq!(item.insert_text.as_deref(), Some("titled"));
        assert_eq!(item.detail.as_deref(), Some("titled"));
    }

    /// Note without frontmatter → label equals stem; no detail or insert_text
    /// override needed, but they are still set for consistency.
    #[test]
    fn completion_falls_back_to_stem() {
        use lsp_types::TextDocumentIdentifier;

        let mut idx = NoteIndex::default();
        idx.index(note("/vault/plain.md", "Body with no frontmatter.\n"));
        idx.index(note("/vault/cursor.md", "[["));

        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: file_uri("/vault/cursor.md") },
                position: Position { line: 0, character: 2 },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };
        let items = handle_completion(params, &idx);
        let item = items
            .iter()
            .find(|i| i.filter_text.as_deref() == Some("plain"))
            .expect("item for plain.md not found");
        assert_eq!(item.label, "plain");
        assert_eq!(item.insert_text.as_deref(), Some("plain"));
        assert_eq!(item.detail, None);
    }

    // ── hover ─────────────────────────────────────────────────────────────────

    fn hover_params(path: &str, line: u32, character: u32) -> HoverParams {
        use lsp_types::TextDocumentIdentifier;
        HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
        }
    }

    /// Resolved wiki-link on a note with a frontmatter title → hover contains
    /// the bold title.
    #[test]
    fn hover_wiki_link_resolved() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", "---\ntitle: B Note\n---\nSome content.\n"));
        idx.index(note("/vault/a.md", "[[b]]"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 2), &idx)
            .expect("expected a hover result");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup hover contents");
        };
        assert!(mc.value.contains("**B Note**"), "expected bold title: {}", mc.value);
    }

    /// Broken wiki-link → `None`.
    #[test]
    fn hover_wiki_link_broken_returns_none() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "[[missing]]"));

        assert!(handle_hover(hover_params("/vault/a.md", 0, 3), &idx).is_none());
    }

    /// Target with more than PREVIEW_LINES body lines → body truncated with `…`.
    #[test]
    fn hover_wiki_link_shows_preview_lines() {
        let body: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", &body));
        idx.index(note("/vault/a.md", "[[b]]"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 2), &idx)
            .expect("expected hover");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup");
        };
        assert!(mc.value.contains('\u{2026}'), "expected truncation marker");
        assert!(mc.value.contains("line 10"), "line 10 should be present");
        assert!(!mc.value.contains("line 11"), "line 11 should be truncated");
    }

    /// Target with frontmatter → hover body omits the `---` delimiters.
    #[test]
    fn hover_wiki_link_skips_frontmatter() {
        let content = "---\ntitle: My Note\n---\nBody line here.\n";
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/b.md", content));
        idx.index(note("/vault/a.md", "[[b]]"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 2), &idx)
            .expect("expected hover");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup");
        };
        assert!(!mc.value.contains("---"), "frontmatter delimiters must not appear: {}", mc.value);
        assert!(mc.value.contains("Body line here."), "body must appear: {}", mc.value);
    }

    /// Cursor not on any link → `None`.
    #[test]
    fn hover_off_link_returns_none() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "plain text [[b]]"));

        assert!(handle_hover(hover_params("/vault/a.md", 0, 0), &idx).is_none());
    }

    /// External URL Markdown link → formatted `[text](url)` hover.
    #[test]
    fn hover_md_link_external_url() {
        let mut idx = NoteIndex::default();
        // "[text](https://example.com)" is 28 chars; (0,5) is inside
        idx.index(note("/vault/a.md", "[text](https://example.com)"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 5), &idx)
            .expect("expected hover for external URL");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup");
        };
        assert_eq!(mc.value, "[text](https://example.com)");
    }

    /// Local relative link that resolves to an indexed note → note preview.
    #[test]
    fn hover_md_link_local_note() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/other.md", "---\ntitle: Other\n---\nContent here.\n"));
        // "[text](./other.md)" is 19 chars; path.parent() = /vault,
        // normalized resolved = /vault/other.md which is indexed.
        idx.index(note("/vault/a.md", "[text](./other.md)"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 5), &idx)
            .expect("expected hover for local note link");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup");
        };
        assert!(mc.value.contains("**Other**"), "expected note title: {}", mc.value);
        assert!(mc.value.contains("Content here."), "expected note body: {}", mc.value);
    }

    /// Image link → "**Image**" header with the path.
    #[test]
    fn hover_md_link_image() {
        let mut idx = NoteIndex::default();
        // "![alt](img.png)" is 15 chars; (0,3) is inside
        idx.index(note("/vault/a.md", "![alt](img.png)"));

        let hover = handle_hover(hover_params("/vault/a.md", 0, 3), &idx)
            .expect("expected hover for image");
        let HoverContents::Markup(mc) = hover.contents else {
            panic!("expected Markup");
        };
        assert!(mc.value.contains("**Image**"), "expected Image header: {}", mc.value);
        assert!(mc.value.contains("img.png"), "expected path: {}", mc.value);
    }

    // ── tag completion ────────────────────────────────────────────────────────

    fn completion_params_at(path: &str, line: u32, character: u32) -> CompletionParams {
        use lsp_types::TextDocumentIdentifier;
        CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        }
    }

    /// Cursor on the `tags: [` line → tag completions.
    #[test]
    fn completion_tag_inline_trigger() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
        // b.md has tags: [lsp], cursor on the tags: [ line at col 9
        idx.index(note("/vault/b.md", "---\ntags: [lsp]\n---\n"));
        // cursor.md has the trigger line
        idx.index(note("/vault/cursor.md", "---\ntags: [\n---\n"));

        let params = completion_params_at("/vault/cursor.md", 1, 8);
        let items = handle_completion(params, &idx);
        assert!(!items.is_empty(), "expected tag completions");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"rust"), "expected 'rust': {:?}", labels);
        assert!(labels.contains(&"lsp"), "expected 'lsp': {:?}", labels);
    }

    /// Cursor on a `- ` list item following a bare `tags:` key → tag completions.
    #[test]
    fn completion_tag_block_trigger() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
        // cursor.md uses block list form
        idx.index(note("/vault/cursor.md", "---\ntags:\n  - \n---\n"));

        let params = completion_params_at("/vault/cursor.md", 2, 4);
        let items = handle_completion(params, &idx);
        assert!(!items.is_empty(), "expected tag completions for block list item");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"rust"), "expected 'rust': {:?}", labels);
    }

    /// Cursor on `title:` line → no tag completions (different key).
    #[test]
    fn completion_tag_no_trigger_title() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/cursor.md", "---\ntitle: \n---\n"));

        let params = completion_params_at("/vault/cursor.md", 1, 7);
        let items = handle_completion(params, &idx);
        assert!(items.is_empty(), "expected no completions on title: line");
    }

    /// Cursor in body (below frontmatter) → wiki-link trigger path, not tags.
    #[test]
    fn completion_tag_no_trigger_body() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
        idx.index(note("/vault/cursor.md", "---\ntags: [rust]\n---\n[["));

        // line 3 is the body `[[` line — should get wiki-link completions (non-empty)
        // but NOT tag completions. We verify by checking item kinds.
        let params = completion_params_at("/vault/cursor.md", 3, 2);
        let items = handle_completion(params, &idx);
        // All items from wiki-link trigger have kind FILE, not VALUE
        for item in &items {
            assert_ne!(item.kind, Some(CompletionItemKind::VALUE),
                "body line should not produce tag (VALUE) completions");
        }
    }

    /// Tag completions contain all known tags from the index.
    #[test]
    fn completion_tag_items_from_index() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [alpha, beta]\n---\n"));
        idx.index(note("/vault/b.md", "---\ntags: [gamma]\n---\n"));
        idx.index(note("/vault/cursor.md", "---\ntags: [\n---\n"));

        let params = completion_params_at("/vault/cursor.md", 1, 8);
        let items = handle_completion(params, &idx);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"alpha"));
        assert!(labels.contains(&"beta"));
        assert!(labels.contains(&"gamma"));
    }

    // ── tag definition / references ───────────────────────────────────────────

    fn make_definition_params_at(path: &str, line: u32, character: u32) -> GotoDefinitionParams {
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

    /// Cursor on a tag → definition returns all notes carrying that tag.
    #[test]
    fn definition_tag_returns_all_locations() {
        let mut idx = NoteIndex::default();
        // "---\ntags: [rust]\n---\n"
        //  line 1: tags: [rust]
        //  'rust' starts at col 8 on line 1
        idx.index(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
        idx.index(note("/vault/b.md", "---\ntags: [rust, lsp]\n---\n"));
        idx.index(note("/vault/c.md", "---\ntags: [lsp]\n---\n")); // no rust

        // Cursor on 'rust' in a.md: line 1, char 8
        let params = make_definition_params_at("/vault/a.md", 1, 8);
        let resp = handle_definition(params, &idx).expect("expected a response");
        let GotoDefinitionResponse::Array(locs) = resp else {
            panic!("expected Array response for tag definition");
        };
        assert_eq!(locs.len(), 2, "expected two notes with 'rust' tag: {:?}", locs);
        let uris: Vec<&str> = locs.iter().map(|l| l.uri.as_str()).collect();
        assert!(uris.iter().any(|u| u.ends_with("a.md")));
        assert!(uris.iter().any(|u| u.ends_with("b.md")));
    }

    /// Tag matching is case-insensitive.
    #[test]
    fn definition_tag_case_insensitive() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [Rust]\n---\n"));
        idx.index(note("/vault/b.md", "---\ntags: [rust]\n---\n"));

        // Cursor on 'Rust' in a.md (line 1, char 8)
        let params = make_definition_params_at("/vault/a.md", 1, 8);
        let resp = handle_definition(params, &idx).expect("expected a response");
        let GotoDefinitionResponse::Array(locs) = resp else {
            panic!("expected Array");
        };
        assert_eq!(locs.len(), 2, "case-insensitive: both notes should match");
    }

    /// Wiki-link definition still returns Scalar (unchanged behaviour).
    #[test]
    fn definition_wiki_link_unchanged() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/target.md", ""));
        idx.index(note("/vault/src.md", "[[target]]\n"));

        let params = make_definition_params_at("/vault/src.md", 0, 3);
        let resp = handle_definition(params, &idx).expect("expected a response");
        assert!(matches!(resp, GotoDefinitionResponse::Scalar(_)),
            "wiki-link definition should still return Scalar");
    }

    /// Cursor on a tag → references returns the same set as definition.
    #[test]
    fn references_tag_returns_all_locations() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntags: [rust]\n---\n"));
        idx.index(note("/vault/b.md", "---\ntags: [rust]\n---\n"));

        // Cursor on 'rust' in a.md: line 1, char 8
        let params = make_references_params("/vault/a.md", 1, 8);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 2, "expected references for both notes with 'rust'");
    }

    /// Cursor on non-tag frontmatter text → no tag result.
    #[test]
    fn tag_at_position_miss() {
        let mut idx = NoteIndex::default();
        idx.index(note("/vault/a.md", "---\ntitle: My Note\ntags: [rust]\n---\n"));

        // Cursor on 'title:' line — not a tag
        let params = make_definition_params_at("/vault/a.md", 1, 3);
        assert!(handle_definition(params, &idx).is_none(), "title: line should not match tags");
    }

    // ── UTF-16 trigger correctness ────────────────────────────────────────────

    /// `[[` after a multi-byte character (e.g. `é` = 1 UTF-16 unit, 2 UTF-8
    /// bytes). pos.character is a UTF-16 unit count, not a byte count.
    #[test]
    fn check_trigger_unicode_prefix() {
        // "café [[" — é is 2 UTF-8 bytes but 1 UTF-16 unit
        // UTF-16 offsets: c=1 a=2 f=3 é=4 space=5 [=6 [=7
        assert!(check_trigger("café [[", Position { line: 0, character: 7 }));
    }

    /// `[[` fires correctly when the cursor is right after it with emoji prefix.
    #[test]
    fn check_trigger_emoji_prefix() {
        // "🎉 [[" — emoji is 2 UTF-16 units, 4 UTF-8 bytes
        // UTF-16 offsets: 🎉=2 space=3 [=4 [=5
        assert!(check_trigger("🎉 [[", Position { line: 0, character: 5 }));
    }

    /// Cursor one unit short of `[[` → no trigger.
    #[test]
    fn check_trigger_unicode_prefix_short() {
        // "café [" — cursor at UTF-16 offset 6, only one `[`
        assert!(!check_trigger("café [[", Position { line: 0, character: 6 }));
    }
}
