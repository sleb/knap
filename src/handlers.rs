use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crossbeam_channel::Sender;
use log::warn;
use lsp_server::{Message, Notification};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeLens, CodeLensParams, Command,
    CompletionItem, CompletionItemKind, CompletionParams, CompletionTextEdit, CreateFile,
    CreateFileOptions, Diagnostic, DiagnosticSeverity, DocumentChangeOperation, DocumentChanges,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams,
    Location, Position, PrepareRenameResponse, PublishDiagnosticsParams, Range, ReferenceParams,
    RenameFilesParams, RenameParams, ResourceOp, SymbolInformation, SymbolKind,
    TextDocumentPositionParams, TextEdit, WorkspaceEdit, WorkspaceSymbolParams,
};

use crate::index::{self, NoteIndex, ResolvedLink};
use crate::parser;

// ─── GFM slug ─────────────────────────────────────────────────────────────────

/// Convert heading text to a GitHub Flavored Markdown anchor slug.
/// `## My Section` → `"my-section"`, `## Hello, World!` → `"hello-world"`.
fn slug(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .to_lowercase()
        .replace(' ', "-")
}

// ─── Diagnostics ──────────────────────────────────────────────────────────────

const DIAG_SOURCE: &str = "knap";

/// Compute LSP diagnostics for `path` against the current index state.
pub(crate) fn compute_diagnostics(path: &Path, index: &NoteIndex, config: &crate::server::Config) -> Vec<Diagnostic> {
    let Some(note) = index.get_note(path) else {
        return vec![];
    };

    let mut diagnostics = Vec::new();

    for link in &note.md_links {
        if link.target.is_empty() {
            // Bare anchor (`[text](#slug)`): validate against current note's headings.
            // `[text](#)` has `link.anchor = None` (empty slug) — nothing to check.
            let Some(anchor) = &link.anchor else { continue };
            let found = note.headings.iter().any(|h| slug(&h.text) == slug(anchor));
            if !found {
                let range = link.anchor_range.unwrap_or(link.range);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Heading not found: '#{anchor}'"),
                    source: Some(DIAG_SOURCE.to_owned()),
                    ..Default::default()
                });
            }
            continue;
        }
        match index.resolve(path, &link.target) {
            ResolvedLink::Broken => {
                diagnostics.push(Diagnostic {
                    range: link.target_range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Link target not found: '{}'", link.target),
                    source: Some(DIAG_SOURCE.to_owned()),
                    ..Default::default()
                });
            }
            ResolvedLink::Found(target_path) => {
                if let Some(anchor) = &link.anchor {
                    if index::is_url_like(&link.target) {
                        continue; // can't verify headings on remote URLs
                    }
                    let found = index
                        .get_note(&target_path)
                        .map(|n| {
                            n.headings
                                .iter()
                                .any(|h| slug(&h.text) == slug(anchor))
                        })
                        .unwrap_or(false);
                    if !found {
                        let range = link.anchor_range.unwrap_or(link.range);
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!("Heading not found: '#{anchor}'"),
                            source: Some(DIAG_SOURCE.to_owned()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    let schema = &config.frontmatter_schema;
    if !schema.fields.is_empty() || schema.require_frontmatter || schema.warn_unknown_keys {
        let zero = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        };
        match &note.frontmatter {
            None => {
                if schema.require_frontmatter {
                    for (key, sf) in &schema.fields {
                        if sf.required {
                            diagnostics.push(Diagnostic {
                                range: zero,
                                severity: Some(DiagnosticSeverity::WARNING),
                                message: format!("Required frontmatter key missing: '{key}'"),
                                source: Some(DIAG_SOURCE.to_owned()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            Some(fm) => {
                for (key, sf) in &schema.fields {
                    if sf.required
                        && !fm.fields.iter().any(|f| f.key.eq_ignore_ascii_case(key))
                    {
                        diagnostics.push(Diagnostic {
                            range: zero,
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: format!("Required frontmatter key missing: '{key}'"),
                            source: Some(DIAG_SOURCE.to_owned()),
                            ..Default::default()
                        });
                    }
                }
                for field in &fm.fields {
                    match schema.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(&field.key)) {
                        Some((_, sf)) => {
                            if let (Some(allowed), Some(value), Some(value_range)) =
                                (&sf.values, &field.value, field.value_range)
                                && !allowed.iter().any(|v| v == value)
                            {
                                diagnostics.push(Diagnostic {
                                    range: value_range,
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!(
                                        "Value '{value}' is not in the allowed list for '{}'",
                                        field.key
                                    ),
                                    source: Some(DIAG_SOURCE.to_owned()),
                                    ..Default::default()
                                });
                            }
                        }
                        None => {
                            if schema.warn_unknown_keys {
                                diagnostics.push(Diagnostic {
                                    range: field.key_range,
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!(
                                        "Unknown frontmatter key: '{}'",
                                        field.key
                                    ),
                                    source: Some(DIAG_SOURCE.to_owned()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

/// Publish `textDocument/publishDiagnostics` notifications for every path in `paths`.
pub(crate) fn publish_diagnostics(paths: &HashSet<PathBuf>, index: &NoteIndex, config: &crate::server::Config, sender: &Sender<Message>) {
    for path in paths {
        let diagnostics = compute_diagnostics(path, index, config);
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

// ─── Tag helpers ──────────────────────────────────────────────────────────────

fn find_tag_at_position(note: &parser::Note, pos: Position) -> Option<&parser::Tag> {
    let fm = note.frontmatter.as_ref()?;
    fm.tags.iter().find(|tag| contains(tag.range, pos))
}

/// Returns `Some((partial, replace_range))` when the cursor is inside a
/// frontmatter `tags:` value position. Supports three YAML forms:
///   - Bare scalar:   `tags: partial`
///   - Inline list:   `tags: [rust, partial`
///   - Block list:    `  - partial` (under a `tags:` header)
///
/// `partial` is the tag text already typed; `replace_range` covers it so the
/// completion text edit replaces it cleanly.
fn check_tag_trigger(content: &str, pos: Position) -> Option<(String, Range)> {
    let lines: Vec<&str> = content.lines().collect();

    // Must start with a frontmatter block.
    if lines.first().copied() != Some("---") {
        return None;
    }
    // Find the closing `---` line (scanning from line 1).
    let closing_line = lines[1..].iter().position(|l| l.trim() == "---")? + 1;

    // Cursor must be inside the frontmatter (not on either `---` marker line).
    if pos.line == 0 || pos.line as usize >= closing_line {
        return None;
    }

    let line = lines[pos.line as usize];
    let cursor_byte = utf16_to_byte_offset(line, pos.character);

    // ── Bare scalar or inline list: line starts with `tags:` ──────────────────
    if let Some(after_colon) = line.strip_prefix("tags:") {
        let trimmed = after_colon.trim_start();

        if trimmed.starts_with('[') {
            // Inline list: `tags: [rust, partial`
            let bracket_byte = line.find('[').expect("found [ via trim");
            if cursor_byte <= bracket_byte {
                return None;
            }
            // Check cursor is not past the closing `]`.
            if matches!(line.find(']'), Some(close) if cursor_byte > close) {
                return None;
            }
            let content_before = &line[bracket_byte + 1..cursor_byte];
            let (partial, partial_offset_in_content) = match content_before.rfind(',') {
                Some(comma) => {
                    let after = &content_before[comma + 1..];
                    let ws = after.len() - after.trim_start().len();
                    (after.trim_start().to_string(), comma + 1 + ws)
                }
                None => {
                    let ws = content_before.len() - content_before.trim_start().len();
                    (content_before.trim_start().to_string(), ws)
                }
            };
            let partial_start = bracket_byte + 1 + partial_offset_in_content;
            let start_char = byte_to_utf16_offset(line, partial_start);
            return Some((partial, Range {
                start: Position { line: pos.line, character: start_char },
                end: pos,
            }));
        }

        if !trimmed.starts_with('-') {
            // Bare scalar: `tags: partial`
            let ws = after_colon.len() - after_colon.trim_start().len();
            let value_start = "tags:".len() + ws;
            if cursor_byte < value_start {
                return None;
            }
            let partial = line[value_start..cursor_byte].to_string();
            let start_char = byte_to_utf16_offset(line, value_start);
            return Some((partial, Range {
                start: Position { line: pos.line, character: start_char },
                end: pos,
            }));
        }
    }

    // ── Block list item: `  - partial` ────────────────────────────────────────
    let stripped = line.trim_start();
    if let Some(after_dash) = stripped.strip_prefix('-') {
        let leading_ws = line.len() - stripped.len();
        // Backtrack through sibling list items to find a bare `tags:` header.
        let mut found_tags = false;
        for scan in (1..pos.line as usize).rev() {
            let sl = lines[scan].trim_start();
            if sl.starts_with('-') {
                continue; // sibling list item
            }
            if lines[scan].trim() == "tags:" {
                found_tags = true;
            }
            break;
        }
        if !found_tags {
            return None;
        }
        let ws_after_dash = after_dash.len() - after_dash.trim_start().len();
        let partial_start = leading_ws + 1 + ws_after_dash; // past `- ` prefix
        if cursor_byte < partial_start {
            return None;
        }
        let partial = line[partial_start..cursor_byte].to_string();
        let start_char = byte_to_utf16_offset(line, partial_start);
        return Some((partial, Range {
            start: Position { line: pos.line, character: start_char },
            end: pos,
        }));
    }

    None
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

fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
}

/// Returns `Some(partial)` when the cursor is inside a Markdown link destination
/// (there is a `](` before the cursor on the same line and no `#` between `](`
/// and the cursor). `partial` is the text typed so far between `](` and the
/// cursor — empty immediately after `](`, non-empty while typing a path.
/// Returns `None` outside a link destination or inside an anchor context (`#`).
fn check_dir_trigger(content: &str, pos: Position) -> Option<String> {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    let before = &line[..cursor];
    let open = before.rfind("](")?;
    let after_open = &before[open + 2..];
    if after_open.contains('#') {
        return None;
    }
    Some(after_open.to_string())
}

/// If the cursor is immediately after a `#` inside a link destination
/// (`](path#`), return the path segment between `](` and `#`.
/// Returns `None` if the context doesn't match or the path is empty.
fn check_anchor_trigger(content: &str, pos: Position) -> Option<String> {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    let before = &line[..cursor];
    let open = before.rfind("](")?;
    let after_open = &before[open + 2..];
    let hash_pos = after_open.find('#')?;
    let path = &after_open[..hash_pos];
    Some(path.to_string())
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

/// Returns the line text and cursor byte offset when `pos` falls inside a
/// frontmatter block (between the opening and closing `---` delimiters,
/// exclusive). Returns `None` when the file has no frontmatter, or the cursor
/// is on a delimiter line, or the closing delimiter is absent.
fn frontmatter_cursor_line(content: &str, pos: Position) -> Option<(&str, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first().copied() != Some("---") {
        return None;
    }
    let closing_line = lines[1..].iter().position(|l| l.trim() == "---")? + 1;
    if pos.line == 0 || pos.line as usize >= closing_line {
        return None;
    }
    let line = lines[pos.line as usize];
    let cursor_byte = utf16_to_byte_offset(line, pos.character);
    Some((line, cursor_byte))
}

/// Returns `Some((key, partial, replace_range))` when the cursor is inside a
/// frontmatter value position (after the `:` on a key-value line).
/// Returns `None` for the `tags:` key (handled by `check_tag_trigger`),
/// for complex values (inline list, block scalar), or outside the frontmatter block.
fn check_frontmatter_value_trigger(content: &str, pos: Position) -> Option<(String, String, Range)> {
    let (line, cursor_byte) = frontmatter_cursor_line(content, pos)?;
    let colon_byte = line.find(':')?;
    if cursor_byte <= colon_byte {
        return None;
    }

    let key = line[..colon_byte].trim().to_string();
    if key.is_empty() || key.starts_with('#') {
        return None;
    }
    if line.starts_with("tags:") {
        return None;
    }

    let after_colon = &line[colon_byte + 1..];
    let trimmed_value = after_colon.trim_start();
    if trimmed_value.starts_with('[') || trimmed_value.starts_with('|') || trimmed_value.starts_with('>') {
        return None;
    }

    let ws = after_colon.len() - after_colon.trim_start().len();
    let value_start = colon_byte + 1 + ws;
    let partial = if cursor_byte >= value_start {
        line[value_start..cursor_byte].to_string()
    } else {
        String::new()
    };

    let start_char = byte_to_utf16_offset(line, value_start);
    let replace_range = Range {
        start: Position { line: pos.line, character: start_char },
        end: pos,
    };
    Some((key, partial, replace_range))
}

/// Returns `Some((partial, replace_range))` when the cursor is inside a
/// frontmatter key position (before or on the `:` of a key-value line, or on
/// a blank frontmatter line). Returns `None` for list items, comment lines,
/// value positions, or outside the frontmatter block.
fn check_frontmatter_key_trigger(content: &str, pos: Position) -> Option<(String, Range)> {
    let (line, cursor_byte) = frontmatter_cursor_line(content, pos)?;
    let trimmed = line.trim_start();
    if trimmed.starts_with('-') || trimmed.starts_with('#') {
        return None;
    }
    if let Some(colon_byte) = line.find(':')
        && cursor_byte > colon_byte
    {
        return None;
    }

    let leading_ws = line.len() - trimmed.len();
    let partial = if cursor_byte >= leading_ws {
        line[leading_ws..cursor_byte].to_string()
    } else {
        String::new()
    };
    let start_char = byte_to_utf16_offset(line, leading_ws);
    let replace_range = Range {
        start: Position { line: pos.line, character: start_char },
        end: pos,
    };
    Some((partial, replace_range))
}

fn heading_completion_item(h: &parser::Heading) -> CompletionItem {
    let s = slug(&h.text);
    CompletionItem {
        label: h.text.clone(),
        kind: Some(CompletionItemKind::REFERENCE),
        filter_text: Some(h.text.clone()),
        insert_text: Some(s.clone()),
        detail: Some(format!("#{s}")),
        ..Default::default()
    }
}

/// Handle `textDocument/completion`: link paths, anchors, and tag values.
pub(crate) fn handle_completion(params: CompletionParams, index: &NoteIndex, config: &crate::server::Config) -> Vec<CompletionItem> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };

    // Tag completion: cursor is inside a frontmatter `tags:` value.
    if let Some((partial, replace_range)) = check_tag_trigger(&note.content, pos) {
        let used: std::collections::HashSet<String> = note.frontmatter
            .as_ref()
            .map(|fm| fm.tags.iter().map(|t| t.name.to_lowercase()).collect())
            .unwrap_or_default();
        return index
            .all_tags()
            .filter(|t| !used.contains(*t))
            .filter(|t| t.starts_with(&partial.to_lowercase()))
            .map(|tag_name| CompletionItem {
                label: tag_name.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: tag_name.to_string(),
                })),
                ..Default::default()
            })
            .collect();
    }

    // Frontmatter value completion: cursor is in a scalar value position.
    if let Some((key, partial, replace_range)) = check_frontmatter_value_trigger(&note.content, pos) {
        let schema = &config.frontmatter_schema;
        if let Some((_, sf)) = schema.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key))
            && let Some(allowed) = &sf.values
        {
            return allowed
                .iter()
                .filter(|v| v.starts_with(&partial as &str))
                .map(|v| CompletionItem {
                    label: v.clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace_range,
                        new_text: v.clone(),
                    })),
                    ..Default::default()
                })
                .collect();
        }
        return vec![];
    }

    // Anchor completion: `](path#` → list headings from target note,
    // or `](#` → list headings from the current note.
    if let Some(target_rel) = check_anchor_trigger(&note.content, pos) {
        if target_rel.is_empty() {
            return note
                .headings
                .iter()
                .map(heading_completion_item)
                .collect();
        }
        let ResolvedLink::Found(target_path) = index.resolve(&path, &target_rel) else {
            return vec![];
        };
        let Some(target_note) = index.get_note(&target_path) else {
            return vec![];
        };
        return target_note
            .headings
            .iter()
            .map(heading_completion_item)
            .collect();
    }

    // Directory completion: `](` or `](partial/` → immediate children of base_dir.
    if let Some(partial) = check_dir_trigger(&note.content, pos) {
    let note_dir = path.parent().expect("indexed path must have a parent");
    let base_dir = if partial.ends_with('/') || partial.is_empty() {
        index::normalize_path(&note_dir.join(&*partial))
    } else {
        let p = std::path::Path::new(&partial);
        index::normalize_path(&note_dir.join(p.parent().unwrap_or(std::path::Path::new(""))))
    };

    // Collect owned copies so we can borrow index freely afterwards.
    let note_paths: Vec<PathBuf> = index.all_notes().map(|n| n.path.clone()).collect();
    let attach_paths: Vec<PathBuf> = index.all_attachment_paths().map(Path::to_path_buf).collect();

    let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut files: Vec<PathBuf> = vec![];

    for file_path in note_paths.iter().chain(attach_paths.iter()) {
        if file_path.as_path() == path.as_path() {
            continue;
        }
        let rel = relative_path(&base_dir, file_path);
        let first = rel.split('/').next().unwrap_or("");
        if first == rel && !rel.is_empty() {
            files.push(file_path.clone());
        } else if !first.is_empty() {
            dirs.insert(first.to_string());
        }
    }

    // Compute the TextEdit range: from right after `](` to the cursor.
    let line_text = note.content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor_byte = utf16_to_byte_offset(line_text, pos.character);
    let open_byte = line_text[..cursor_byte]
        .rfind("](")
        .expect("check_dir_trigger guarantees ](");
    let start_char = byte_to_utf16_offset(line_text, open_byte + 2);
    let replace_range = Range {
        start: Position { line: pos.line, character: start_char },
        end: pos,
    };

    let mut items: Vec<CompletionItem> = Vec::new();

    for dir_name in &dirs {
        let abs_dir = index::normalize_path(&base_dir.join(dir_name));
        let full_rel = relative_path(note_dir, &abs_dir) + "/";
        items.push(CompletionItem {
            label: format!("{dir_name}/"),
            kind: Some(CompletionItemKind::FOLDER),
            filter_text: Some(dir_name.clone()),
            sort_text: Some(format!("0_{dir_name}")),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: full_rel,
            })),
            ..Default::default()
        });
    }

    // Track immediate file paths so we can skip them in the global list.
    let immediate_set: std::collections::HashSet<&PathBuf> = files.iter().collect();

    for file_path in &files {
        let full_rel = relative_path(note_dir, file_path);
        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| full_rel.clone());
        let (label, detail) = match index.get_note(file_path) {
            Some(n) => {
                let title = n.frontmatter.as_ref().and_then(|fm| fm.title.clone());
                (title.clone().unwrap_or_else(|| file_name.clone()), title.map(|_| file_name.clone()))
            }
            None => (file_name.clone(), None),
        };
        items.push(CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            filter_text: Some(file_name.clone()),
            sort_text: Some(format!("1_{file_name}")),
            detail,
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: full_rel,
            })),
            ..Default::default()
        });
    }

    // Global items: every workspace file not already shown as an immediate child.
    // These let the user jump directly to any file without drilling through dirs.
    for file_path in note_paths.iter().chain(attach_paths.iter()) {
        if file_path.as_path() == path.as_path() {
            continue;
        }
        if immediate_set.contains(file_path) {
            continue;
        }
        let full_rel = relative_path(note_dir, file_path);
        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| full_rel.clone());
        let label = match index.get_note(file_path) {
            Some(n) => n
                .frontmatter
                .as_ref()
                .and_then(|fm| fm.title.clone())
                .unwrap_or_else(|| file_name.clone()),
            None => file_name.clone(),
        };
        items.push(CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            filter_text: Some(full_rel.clone()),
            sort_text: Some(format!("2_{full_rel}")),
            detail: Some(full_rel.clone()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: full_rel,
            })),
            ..Default::default()
        });
    }

    return items;
    }

    // Frontmatter key completion: cursor is in a key position inside the frontmatter block.
    if let Some((partial, replace_range)) = check_frontmatter_key_trigger(&note.content, pos) {
        let schema = &config.frontmatter_schema;
        if !schema.fields.is_empty() {
            let used: HashSet<String> = note
                .frontmatter
                .as_ref()
                .map(|fm| fm.fields.iter().map(|f| f.key.to_lowercase()).collect())
                .unwrap_or_default();
            return schema
                .fields
                .iter()
                .filter(|(k, _)| !used.contains(&k.to_lowercase()))
                .filter(|(k, _)| k.to_lowercase().starts_with(&partial.to_lowercase()))
                .map(|(key, _)| CompletionItem {
                    label: key.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace_range,
                        new_text: format!("{key}: "),
                    })),
                    ..Default::default()
                })
                .collect();
        }
    }

    vec![]
}

// ─── Tag locations (shared by definition and references) ──────────────────────

/// All locations in the workspace where `tag_name` appears in frontmatter.
fn tag_locations(tag_name: &str, index: &NoteIndex) -> Vec<Location> {
    index
        .notes_by_tag(tag_name)
        .flat_map(|n| {
            let uri = path_to_uri(&n.path);
            n.frontmatter
                .iter()
                .flat_map(|fm| fm.tags.iter())
                .filter(|t| t.name.eq_ignore_ascii_case(tag_name))
                .map(|t| Location { uri: uri.clone(), range: t.range })
                .collect::<Vec<_>>()
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

fn find_md_link_at_position(
    note: &crate::parser::Note,
    pos: Position,
) -> Option<&crate::parser::MarkdownLink> {
    note.md_links.iter().find(|link| contains(link.range, pos))
}

fn find_heading_at_position(note: &parser::Note, pos: Position) -> Option<&parser::Heading> {
    note.headings
        .iter()
        .find(|h| h.range.start.line <= pos.line && pos.line <= h.range.end.line)
}

/// Handle `textDocument/definition`: navigate to a link's target or a tag's occurrences.
pub(crate) fn handle_definition(
    params: GotoDefinitionParams,
    index: &NoteIndex,
) -> Option<GotoDefinitionResponse> {
    let pos = params.text_document_position_params.position;
    let path = uri_to_path(&params.text_document_position_params.text_document.uri)?;
    let note = index.get_note(&path)?;

    // Tag: go-to-definition shows every note that carries the tag.
    if let Some(tag) = find_tag_at_position(note, pos) {
        let tag_name = tag.name.clone();
        let locations: Vec<Location> = tag_locations(&tag_name, index);
        return Some(GotoDefinitionResponse::Array(locations));
    }

    let link = find_md_link_at_position(note, pos)?;

    // Bare anchor (`[text](#slug)`): resolve against current note's headings.
    if link.target.is_empty() {
        let range = link
            .anchor
            .as_ref()
            .and_then(|anchor| note.headings.iter().find(|h| slug(&h.text) == slug(anchor)))
            .map(|h| h.range)
            .unwrap_or_default();
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: path_to_uri(&path),
            range,
        }));
    }

    let ResolvedLink::Found(target_path) = index.resolve(&path, &link.target) else {
        return None;
    };
    let anchor_range = link.anchor.as_ref().and_then(|anchor| {
        let target_note = index.get_note(&target_path)?;
        let heading = target_note
            .headings
            .iter()
            .find(|h| slug(&h.text) == slug(anchor))?;
        Some(heading.range)
    });
    let range = anchor_range.unwrap_or_default();
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: path_to_uri(&target_path),
        range,
    }))
}

// ─── Find References ──────────────────────────────────────────────────────────

/// Handle `textDocument/references`: backlinks to the current file or tag occurrences.
pub(crate) fn handle_references(params: ReferenceParams, index: &NoteIndex) -> Vec<Location> {
    let pos = params.text_document_position.position;
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };

    // Tag: find-references shows every note that carries the tag.
    if let Some(tag) = find_tag_at_position(note, pos) {
        let tag_name = tag.name.clone();
        return tag_locations(&tag_name, index);
    }

    // Link: cursor on a link → backlinks to the link's target file.
    if let Some(link) = find_md_link_at_position(note, pos) {
        let target_path = match index.resolve(&path, &link.target) {
            ResolvedLink::Found(p) => p,
            ResolvedLink::Broken => return vec![],
        };
        return index
            .links_to(&target_path)
            .iter()
            .map(|located| Location {
                uri: path_to_uri(&located.source_path),
                range: located.md_link.range,
            })
            .collect();
    }

    // Heading: cursor on a heading → anchor references to that heading.
    if let Some(heading) = find_heading_at_position(note, pos) {
        let heading_slug = slug(&heading.text);
        let mut locs: Vec<Location> = Vec::new();
        for link in &note.md_links {
            if link.target.is_empty()
                && link.anchor.as_deref().map(slug).as_deref() == Some(&heading_slug)
            {
                locs.push(Location { uri: path_to_uri(&path), range: link.range });
            }
        }
        for located in index.links_to(&path) {
            if located.md_link.anchor.as_deref().map(slug).as_deref() == Some(&heading_slug) {
                locs.push(Location {
                    uri: path_to_uri(&located.source_path),
                    range: located.md_link.range,
                });
            }
        }
        return locs;
    }

    // Fallback: no link, no heading → backlinks to the current file.
    index
        .links_to(&path)
        .iter()
        .map(|located| Location {
            uri: path_to_uri(&located.source_path),
            range: located.md_link.range,
        })
        .collect()
}

// ─── Rename ───────────────────────────────────────────────────────────────────

#[allow(clippy::mutable_key_type)] // lsp_types::Uri has interior mutability; HashMap<Uri, _> is the LSP-spec type
pub(crate) fn handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit {
    use crate::index::{is_url_like, normalize_path};

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
                if link.target.is_empty() || is_url_like(&link.target) {
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
pub(crate) fn handle_document_symbols(
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
pub(crate) fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation> {
    let query = params.query.to_lowercase();

    let mut symbols: Vec<SymbolInformation> = index
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
        .collect();

    for note in index.all_notes() {
        let Some(fm) = note.frontmatter.as_ref() else { continue };
        let uri = path_to_uri(&note.path);
        let container = note.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        for tag in &fm.tags {
            if query.is_empty() || tag.name.to_lowercase().contains(&query) {
                symbols.push(SymbolInformation {
                    name: tag.name.clone(),
                    kind: SymbolKind::KEY,
                    location: Location { uri: uri.clone(), range: tag.range },
                    container_name: Some(container.clone()),
                    tags: None,
                    deprecated: None,
                });
            }
        }
    }

    symbols
}

// ─── Heading Rename ───────────────────────────────────────────────────────────

pub(crate) fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse> {
    let path = uri_to_path(&params.text_document.uri)?;
    let disk_note;
    let note: &parser::Note = match index.get_note(&path) {
        Some(n) => n,
        None => {
            let content = std::fs::read_to_string(&path).ok()?;
            disk_note = parser::parse(&path, &content);
            &disk_note
        }
    };
    let pos = params.position;
    let heading = note.headings.iter().find(|h| {
        h.range.start.line <= pos.line && pos.line <= h.range.end.line
    })?;
    let placeholder = {
        let line_text = note.content.lines()
            .nth(heading.text_range.start.line as usize)
            .unwrap_or("");
        let start = utf16_to_byte_offset(line_text, heading.text_range.start.character);
        let end = utf16_to_byte_offset(line_text, heading.text_range.end.character);
        line_text[start..end].to_string()
    };
    Some(PrepareRenameResponse::RangeWithPlaceholder { range: heading.text_range, placeholder })
}

#[allow(clippy::mutable_key_type)]
pub(crate) fn handle_rename(params: RenameParams, index: &NoteIndex) -> Option<WorkspaceEdit> {
    let path = uri_to_path(&params.text_document_position.text_document.uri)?;
    let disk_note;
    let note: &parser::Note = match index.get_note(&path) {
        Some(n) => n,
        None => {
            let content = std::fs::read_to_string(&path).ok()?;
            disk_note = parser::parse(&path, &content);
            &disk_note
        }
    };
    let pos = params.text_document_position.position;
    let heading = note
        .headings
        .iter()
        .find(|h| h.range.start.line <= pos.line && pos.line <= h.range.end.line)?;
    let new_name = &params.new_name;
    let old_slug = slug(&heading.text);
    let new_slug = slug(new_name);

    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    // a. Rewrite the heading text itself (human-readable, not slugified).
    changes
        .entry(path_to_uri(&path))
        .or_default()
        .push(TextEdit { range: heading.text_range, new_text: new_name.clone() });

    // b. Anchor-only self-links inside the same file (target == "").
    for link in &note.md_links {
        if !link.target.is_empty() {
            continue;
        }
        if link.anchor.as_deref().map(slug).as_deref() != Some(old_slug.as_str()) {
            continue;
        }
        let Some(anchor_range) = link.anchor_range else { continue };
        changes
            .entry(path_to_uri(&path))
            .or_default()
            .push(TextEdit { range: anchor_range, new_text: new_slug.clone() });
    }

    // c. Incoming links from other files that reference this heading by anchor.
    for located in index.links_to(&path) {
        if located.md_link.anchor.as_deref().map(slug).as_deref()
            != Some(old_slug.as_str())
        {
            continue;
        }
        let Some(anchor_range) = located.md_link.anchor_range else { continue };
        changes
            .entry(path_to_uri(&located.source_path))
            .or_default()
            .push(TextEdit { range: anchor_range, new_text: new_slug.clone() });
    }

    Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

// ─── Code Actions ─────────────────────────────────────────────────────────────

pub(crate) fn handle_code_actions(
    params: CodeActionParams,
    index: &NoteIndex,
    config: &crate::server::Config,
) -> Vec<CodeActionOrCommand> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };
    let cursor = params.range.start;
    let mut actions = Vec::new();

    for link in &note.md_links {
        if link.target.is_empty() {
            continue;
        }
        if !contains(link.range, cursor) {
            continue;
        }
        match index.resolve(&path, &link.target) {
            ResolvedLink::Broken => {
                let new_path = new_note_path(&link.target, &path, config);
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: "Create note".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(WorkspaceEdit {
                        document_changes: Some(DocumentChanges::Operations(vec![
                            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                                uri: path_to_uri(&new_path),
                                options: Some(CreateFileOptions {
                                    ignore_if_exists: Some(true),
                                    overwrite: None,
                                }),
                                annotation_id: None,
                            })),
                        ])),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
            ResolvedLink::Found(target_path) => {
                let (Some(anchor), Some(anchor_range)) = (&link.anchor, link.anchor_range) else {
                    continue;
                };
                let target_note = index.get_note(&target_path);
                let anchor_matches = target_note
                    .map(|n| n.headings.iter().any(|h| slug(&h.text) == slug(anchor)))
                    .unwrap_or(false);
                if !anchor_matches {
                    for heading in target_note.iter().flat_map(|n| &n.headings) {
                        let new_anchor = slug(&heading.text);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Change anchor to \"{new_anchor}\""),
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(WorkspaceEdit {
                                changes: Some(std::collections::HashMap::from([(
                                    path_to_uri(&path),
                                    vec![TextEdit {
                                        range: anchor_range,
                                        new_text: new_anchor,
                                    }],
                                )])),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }
        }
    }

    actions
}

fn new_note_path(link_target: &str, source: &Path, config: &crate::server::Config) -> PathBuf {
    match config.new_note_dir.as_deref().zip(config.index_roots.first()) {
        Some((dir, root)) => {
            let stem = Path::new(link_target).file_name().unwrap_or_default();
            root.join(dir).join(stem)
        }
        None => index::normalize_path(&source.parent().unwrap_or(source).join(link_target)),
    }
}

// ─── Code Lens ────────────────────────────────────────────────────────────────

/// Returns a single `↑ N backlinks` code lens at line 0 for any note that has
/// at least one incoming link. Returns an empty vec for orphan notes.
pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return vec![];
    };
    if index.get_note(&path).is_none() {
        return vec![];
    }
    let backlinks = index.links_to(&path);
    if backlinks.is_empty() {
        return vec![];
    }

    let count = backlinks.len();
    let locations: Vec<Location> = backlinks
        .iter()
        .map(|l| Location {
            uri: path_to_uri(&l.source_path),
            range: l.md_link.range,
        })
        .collect();

    let anchor = Position { line: 0, character: 0 };
    let command = Command {
        title: format!("↑ {} backlink{}", count, if count == 1 { "" } else { "s" }),
        command: "editor.action.showReferences".to_string(),
        arguments: Some(vec![
            serde_json::to_value(path_to_uri(&path)).expect("URI is serializable"),
            serde_json::to_value(anchor).expect("Position is serializable"),
            serde_json::to_value(&locations).expect("Locations are serializable"),
        ]),
    };

    vec![CodeLens {
        range: Range { start: anchor, end: anchor },
        command: Some(command),
        data: None,
    }]
}

// ─── Inlay Hints ──────────────────────────────────────────────────────────────

#[allow(dead_code)] // wired up in Step 3 (src/server/mod.rs)
fn range_contains_position(range: &Range, pos: Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

/// Handle `textDocument/inlayHint`: show the resolved note title after each
/// link target within the requested visible range.
#[allow(dead_code)] // wired up in Step 3 (src/server/mod.rs)
pub(crate) fn handle_inlay_hints(params: InlayHintParams, index: &NoteIndex) -> Vec<InlayHint> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return vec![];
    };
    let Some(note) = index.get_note(&path) else {
        return vec![];
    };

    let mut hints = Vec::new();

    for link in &note.md_links {
        if link.target.is_empty() {
            continue;
        }
        if index::is_url_like(&link.target) {
            continue;
        }
        let ResolvedLink::Found(target_path) = index.resolve(&path, &link.target) else {
            continue;
        };
        let Some(title) = index
            .get_note(&target_path)
            .and_then(|n| n.frontmatter.as_ref())
            .and_then(|fm| fm.title.as_deref())
        else {
            continue;
        };
        let position = link.target_range.end;
        if !range_contains_position(&params.range, position) {
            continue;
        }
        hints.push(InlayHint {
            position,
            label: InlayHintLabel::String(format!("-> {title}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        });
    }

    hints
}

// ─── URI utilities ────────────────────────────────────────────────────────────

/// Convert an LSP URI to an absolute filesystem path.
///
/// Returns `None` for non-`file://` URIs (e.g. `untitled:` or
/// `vscode-notebook-cell:`). Callers should silently skip `None` — there is
/// nothing useful to index or serve for a buffer without a path.
pub(crate) fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}

/// Convert an absolute filesystem path to an LSP URI.
///
/// Panics if `path` is not absolute.
pub(crate) fn path_to_uri(path: &Path) -> lsp_types::Uri {
    url::Url::from_file_path(path)
        .unwrap_or_else(|_| panic!("path_to_uri: path must be absolute, got: {}", path.display()))
        .as_str()
        .parse()
        .expect("file URL should parse as Uri")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{
        CompletionItemKind, CompletionParams, CompletionTextEdit, DocumentSymbolParams,
        DocumentSymbolResponse, GotoDefinitionParams, Position, PrepareRenameResponse,
        ReferenceParams, RenameParams, SymbolKind, TextDocumentPositionParams,
        WorkspaceSymbolParams,
    };

    use super::*;
    use crate::index::NoteIndex;
    use crate::test_helpers::note;

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
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing.md"));
    }

    #[test]
    fn diagnostics_valid_link_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[text](b.md)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_broken_anchor() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Existing\n"));
        idx.seed(note("/vault/a.md", "[text](b.md#Missing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing"));
    }

    #[test]
    fn diagnostics_valid_anchor_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Existing\n"));
        idx.seed(note("/vault/a.md", "[text](b.md#Existing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_anchor_only_skipped() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](#)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty(), "empty anchor slug should not produce diagnostics");
    }

    #[test]
    fn diagnostics_external_url_with_anchor_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](https://example.com/page#section)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty(), "external URLs with # fragments should not produce diagnostics");
    }

    // ── handle_completion ─────────────────────────────────────────────────────

    #[test]
    fn completion_no_trigger_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "hello world"));
        let params = make_completion_params("/vault/a.md", 0, 5);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty());
    }

    fn text_edit_new_text(item: &lsp_types::CompletionItem) -> Option<&str> {
        match item.text_edit.as_ref()? {
            CompletionTextEdit::Edit(te) => Some(&te.new_text),
            _ => None,
        }
    }

    #[test]
    fn completion_trigger_returns_notes() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        // "[link](" → cursor at position 7 (after the `(`)
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(!items.is_empty());
        // b.md is a sibling — appears as a FILE item via text_edit
        assert!(items.iter().any(|i| text_edit_new_text(i) == Some("b.md")));
    }

    #[test]
    fn completion_relative_path_subdirectory() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/sub/b.md", ""));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        // sub/ appears as a FOLDER item for drilling in
        assert!(items.iter().any(|i| {
            i.kind == Some(CompletionItemKind::FOLDER) && i.label == "sub/"
        }));
        // sub/b.md also appears as a global FILE item for jumping directly
        assert!(items.iter().any(|i| text_edit_new_text(i) == Some("sub/b.md")));
    }

    #[test]
    fn completion_title_used_as_label() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        // b.md is a sibling — appears with title as label, filename as filter_text
        let item = items.iter().find(|i| text_edit_new_text(i) == Some("b.md")).unwrap();
        assert_eq!(item.label, "My Note");
        assert_eq!(item.detail.as_deref(), Some("b.md"));
    }

    #[test]
    fn completion_includes_attachments() {
        let mut idx = NoteIndex::default();
        let _ = idx.add_attachment(std::path::PathBuf::from("/vault/img.png"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        // img.png is a sibling attachment — appears as a FILE item
        assert!(items.iter().any(|i| text_edit_new_text(i) == Some("img.png")));
    }

    #[test]
    fn completion_attachment_label_is_filename() {
        let mut idx = NoteIndex::default();
        // Attachment is a sibling — label is the filename
        let _ = idx.add_attachment(std::path::PathBuf::from("/vault/report.pdf"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        let item = items
            .iter()
            .find(|i| text_edit_new_text(i) == Some("report.pdf"))
            .unwrap();
        assert_eq!(item.label, "report.pdf");
        assert_eq!(item.filter_text.as_deref(), Some("report.pdf"));
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

    // ── handle_prepare_rename ─────────────────────────────────────────────────

    fn make_prepare_rename_params(path: &str, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            position: Position { line, character },
        }
    }

    #[test]
    fn prepare_rename_on_heading_returns_text_range_and_placeholder() {
        let mut idx = NoteIndex::default();
        let content = "# My Heading\n";
        let text_range = note("/vault/a.md", content).headings[0].text_range;
        idx.seed(note("/vault/a.md", content));
        let params = make_prepare_rename_params("/vault/a.md", 0, 5);
        let resp = handle_prepare_rename(params, &idx);
        match resp.expect("expected Some") {
            PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                assert_eq!(range, text_range);
                assert_eq!(placeholder, "My Heading");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn prepare_rename_off_heading_returns_none() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Heading\n\nsome prose\n"));
        // line 2 is "some prose", not a heading
        let params = make_prepare_rename_params("/vault/a.md", 2, 0);
        assert!(handle_prepare_rename(params, &idx).is_none());
    }

    #[test]
    fn prepare_rename_note_absent_returns_none() {
        let idx = NoteIndex::default();
        let params = make_prepare_rename_params("/vault/missing.md", 0, 0);
        assert!(handle_prepare_rename(params, &idx).is_none());
    }

    #[test]
    fn prepare_rename_placeholder_is_raw_text() {
        // heading.text is pulldown-cmark rendered ("My Fancy Heading"); the placeholder
        // must be the raw source ("My _Fancy_ Heading") so editors that validate
        // placeholder == text-at-range will accept it.
        let mut idx = NoteIndex::default();
        let content = "## My _Fancy_ Heading\n";
        idx.seed(note("/vault/a.md", content));
        let params = make_prepare_rename_params("/vault/a.md", 0, 5);
        let resp = handle_prepare_rename(params, &idx);
        match resp.expect("expected Some") {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "My _Fancy_ Heading");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn prepare_rename_no_headings_returns_none() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "just prose\n"));
        let params = make_prepare_rename_params("/vault/a.md", 0, 3);
        assert!(handle_prepare_rename(params, &idx).is_none());
    }

    // ── handle_rename ─────────────────────────────────────────────────────────

    fn make_rename_heading_params(path: &str, line: u32, character: u32, new_name: &str) -> RenameParams {
        RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
                position: Position { line, character },
            },
            new_name: new_name.to_string(),
            work_done_progress_params: Default::default(),
        }
    }

    #[test]
    fn rename_heading_edits_text() {
        let content = "# Old Heading\n";
        let text_range = note("/vault/a.md", content).headings[0].text_range;
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", content));
        let params = make_rename_heading_params("/vault/a.md", 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        let changes = edit.changes.unwrap();
        let a_uri = file_uri("/vault/a.md");
        assert!(
            changes[&a_uri].iter().any(|e| e.range == text_range && e.new_text == "New Heading"),
            "heading text_range should be rewritten to new_name (human-readable)"
        );
    }

    #[test]
    fn rename_heading_updates_incoming_anchor() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Old Heading\n"));
        idx.seed(note("/vault/b.md", "[link](a.md#old-heading)"));
        let params = make_rename_heading_params("/vault/a.md", 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        let changes = edit.changes.unwrap();
        let b_uri = file_uri("/vault/b.md");
        assert!(changes.contains_key(&b_uri), "incoming slug anchor in b.md should be updated");
        assert!(changes[&b_uri].iter().any(|e| e.new_text == "new-heading"));
    }

    #[test]
    fn rename_heading_updates_self_anchor() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Old Heading\n\n[link](#old-heading)\n"));
        let params = make_rename_heading_params("/vault/a.md", 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        let changes = edit.changes.unwrap();
        let a_uri = file_uri("/vault/a.md");
        assert!(
            changes[&a_uri].iter().any(|e| e.new_text == "New Heading"),
            "heading text should be updated"
        );
        assert!(
            changes[&a_uri].iter().any(|e| e.new_text == "new-heading"),
            "self-anchor should be updated to slug"
        );
    }

    #[test]
    fn rename_heading_case_insensitive_match() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Old Heading\n"));
        idx.seed(note("/vault/b.md", "[link](a.md#OLD-HEADING)"));
        let params = make_rename_heading_params("/vault/a.md", 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        let changes = edit.changes.unwrap();
        let b_uri = file_uri("/vault/b.md");
        assert!(changes.contains_key(&b_uri), "slug of OLD-HEADING should match Old Heading");
    }

    #[test]
    fn rename_heading_non_matching_anchor_skipped() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Old Heading\n"));
        idx.seed(note("/vault/b.md", "[link](a.md#other-section)"));
        let params = make_rename_heading_params("/vault/a.md", 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        let changes = edit.changes.unwrap();
        let b_uri = file_uri("/vault/b.md");
        assert!(!changes.contains_key(&b_uri), "non-matching anchor should not be updated");
    }

    #[test]
    fn rename_heading_no_heading_at_cursor_none() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Old Heading\n\nsome prose\n"));
        let params = make_rename_heading_params("/vault/a.md", 2, 0, "New Heading");
        assert!(handle_rename(params, &idx).is_none());
    }

    // ── disk-fallback (issue #2) ──────────────────────────────────────────────

    fn write_temp(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).expect("write temp file");
        path
    }

    #[test]
    fn prepare_rename_disk_fallback() {
        let path = write_temp("knap_test_pr_fallback.md", "# My Heading\n");
        let idx = NoteIndex::default();
        let params = make_prepare_rename_params(path.to_str().unwrap(), 0, 5);
        let resp = handle_prepare_rename(params, &idx);
        std::fs::remove_file(&path).ok();
        match resp.expect("expected Some for unindexed on-disk file") {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "My Heading");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn prepare_rename_disk_fallback_off_heading() {
        let path = write_temp("knap_test_pr_fallback_off.md", "# Heading\n\nsome prose\n");
        let idx = NoteIndex::default();
        let params = make_prepare_rename_params(path.to_str().unwrap(), 2, 0);
        let resp = handle_prepare_rename(params, &idx);
        std::fs::remove_file(&path).ok();
        assert!(resp.is_none(), "cursor on prose should return None even with disk fallback");
    }

    #[test]
    fn rename_disk_fallback_edits_heading() {
        let path = write_temp("knap_test_rn_fallback.md", "# Old Heading\n");
        let idx = NoteIndex::default();
        let params = make_rename_heading_params(path.to_str().unwrap(), 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some for unindexed on-disk file");
        std::fs::remove_file(&path).ok();
        let changes = edit.changes.unwrap();
        let uri = path_to_uri(&path);
        let edits = changes.get(&uri).expect("expected edits for the file");
        assert!(
            edits.iter().any(|e| e.new_text == "New Heading"),
            "heading text should be rewritten"
        );
    }

    #[test]
    fn rename_disk_fallback_no_incoming_links() {
        let path = write_temp("knap_test_rn_incoming.md", "# Old Heading\n");
        let idx = NoteIndex::default();
        let params = make_rename_heading_params(path.to_str().unwrap(), 0, 5, "New Heading");
        let edit = handle_rename(params, &idx).expect("expected Some");
        std::fs::remove_file(&path).ok();
        let changes = edit.changes.unwrap();
        // Only the file itself should have edits — no incoming links since the index is empty.
        assert_eq!(changes.len(), 1, "expected edits only for the renamed file, not for other files");
    }

    // ── handle_code_actions ───────────────────────────────────────────────────

    fn make_code_action_params(path: &str, line: u32, character: u32) -> lsp_types::CodeActionParams {
        lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            range: Range {
                start: Position { line, character },
                end: Position { line, character },
            },
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    fn make_config(index_roots: Vec<std::path::PathBuf>, new_note_dir: Option<&str>) -> crate::server::Config {
        crate::server::Config {
            index_roots,
            extensions: vec!["md".to_string()],
            new_note_dir: new_note_dir.map(|s| s.to_string()),
            frontmatter_schema: Default::default(),
        }
    }

    fn extract_create_file(action: &lsp_types::CodeActionOrCommand) -> Option<&lsp_types::CreateFile> {
        match action {
            lsp_types::CodeActionOrCommand::CodeAction(a) => {
                let edit = a.edit.as_ref()?;
                if let Some(lsp_types::DocumentChanges::Operations(ops)) = &edit.document_changes {
                    for op in ops {
                        if let lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Create(cf)) = op {
                            return Some(cf);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    #[test]
    fn code_actions_create_note_for_broken_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 1);
        assert!(extract_create_file(&actions[0]).is_some());
    }

    #[test]
    fn code_actions_no_action_for_valid_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    #[test]
    fn code_actions_no_action_off_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md) prose"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 20);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    #[test]
    fn code_actions_new_note_dir_respected() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], Some("inbox"));
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 1);
        let cf = extract_create_file(&actions[0]).unwrap();
        assert!(cf.uri.as_str().ends_with("/vault/inbox/missing.md"));
    }

    #[test]
    fn code_actions_default_path_relative() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/sub/a.md", "[link](missing.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/sub/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 1);
        let cf = extract_create_file(&actions[0]).unwrap();
        assert!(cf.uri.as_str().ends_with("/vault/sub/missing.md"));
    }

    #[test]
    fn code_actions_create_note_ignore_if_exists() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 1);
        let cf = extract_create_file(&actions[0]).unwrap();
        assert_eq!(cf.options.as_ref().and_then(|o| o.ignore_if_exists), Some(true));
    }

    #[test]
    fn code_actions_skips_anchor_only_links() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](#section)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    #[test]
    fn code_actions_fix_anchor_offers_headings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Introduction\n# Summary\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#nonexistent)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn code_actions_fix_anchor_edit_replaces_range() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Introduction\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#nonexistent)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            lsp_types::CodeActionOrCommand::CodeAction(a) => {
                let changes = a.edit.as_ref().unwrap().changes.as_ref().unwrap();
                let edits: Vec<_> = changes.values().flatten().collect();
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].new_text, "introduction");
            }
            _ => panic!("expected CodeAction"),
        }
    }

    #[test]
    fn code_actions_fix_anchor_no_headings_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "just prose"));
        idx.seed(note("/vault/a.md", "[link](b.md#nonexistent)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    #[test]
    fn code_actions_fix_anchor_valid_anchor_skipped() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Introduction\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#introduction)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    #[test]
    fn code_actions_fix_anchor_no_anchor_range_skipped() {
        // A link with anchor but no anchor_range should produce no actions.
        // We test this indirectly: a plain `[link](b.md)` with no anchor → no anchor actions.
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Introduction\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let config = make_config(vec![std::path::PathBuf::from("/vault")], None);
        let params = make_code_action_params("/vault/a.md", 0, 3);
        let actions = handle_code_actions(params, &idx, &config);
        assert!(actions.is_empty());
    }

    // ── anchor completion (US-45) ─────────────────────────────────────────────

    #[test]
    fn anchor_completion_returns_headings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## My Section\n## Other\n"));
        // "[link](b.md#" — cursor at character 12 (right after `#`)
        idx.seed(note("/vault/a.md", "[link](b.md#"));
        let params = make_completion_params("/vault/a.md", 0, 12);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(!items.is_empty(), "should return heading completions");
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn anchor_completion_label_is_heading_text() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## My Section\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#"));
        let params = make_completion_params("/vault/a.md", 0, 12);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert_eq!(items[0].label, "My Section");
    }

    #[test]
    fn anchor_completion_insert_is_slug() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## My Section\n"));
        idx.seed(note("/vault/a.md", "[link](b.md#"));
        let params = make_completion_params("/vault/a.md", 0, 12);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert_eq!(items[0].insert_text.as_deref(), Some("my-section"));
        assert_eq!(items[0].detail.as_deref(), Some("#my-section"));
        assert_eq!(items[0].filter_text.as_deref(), Some("My Section"));
    }

    #[test]
    fn anchor_completion_unknown_file_empty() {
        let mut idx = NoteIndex::default();
        // "[link](missing.md#" — cursor at character 18
        idx.seed(note("/vault/a.md", "[link](missing.md#"));
        let params = make_completion_params("/vault/a.md", 0, 18);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty(), "unresolvable path should yield no completions");
    }

    #[test]
    fn anchor_completion_no_headings_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "no headings here"));
        idx.seed(note("/vault/a.md", "[link](b.md#"));
        let params = make_completion_params("/vault/a.md", 0, 12);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty(), "file with no headings should yield no completions");
    }

    #[test]
    fn anchor_completion_does_not_fire_on_plain_hash() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Section\n"));
        // Hash in prose, not inside a link destination
        idx.seed(note("/vault/a.md", "some text # not a trigger"));
        // cursor at character 11, right after `#`
        let params = make_completion_params("/vault/a.md", 0, 11);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty(), "hash in prose should not trigger anchor completion");
    }

    // ── check_dir_trigger ─────────────────────────────────────────────────────

    #[test]
    fn check_dir_trigger_empty_after_open() {
        // cursor immediately after `](`
        let result = check_dir_trigger("[x](", Position { line: 0, character: 4 });
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn check_dir_trigger_partial_path() {
        // cursor after the trailing `/`
        let result = check_dir_trigger("[x](subdir/", Position { line: 0, character: 11 });
        assert_eq!(result, Some("subdir/".to_string()));
    }

    #[test]
    fn check_dir_trigger_none_outside_link() {
        // `/` typed on a bare text line — no `](` before it
        let result = check_dir_trigger("some text /", Position { line: 0, character: 11 });
        assert!(result.is_none());
    }

    #[test]
    fn check_dir_trigger_none_in_anchor_context() {
        // `#` after path puts us in anchor territory; dir trigger must yield None
        let result = check_dir_trigger("[x](path#", Position { line: 0, character: 9 });
        assert!(result.is_none());
    }

    // ── dir_completion_* ──────────────────────────────────────────────────────

    #[test]
    fn dir_completion_initial_shows_siblings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(
            items.iter().any(|i| {
                i.kind == Some(CompletionItemKind::FILE) && text_edit_new_text(i) == Some("b.md")
            }),
            "sibling note should appear as FILE item"
        );
    }

    #[test]
    fn dir_completion_initial_shows_subdir() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/subdir/c.md", ""));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(
            items.iter().any(|i| {
                i.kind == Some(CompletionItemKind::FOLDER) && i.label == "subdir/"
            }),
            "subdirectory should appear as FOLDER item"
        );
    }

    #[test]
    fn dir_completion_initial_excludes_current() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty(), "current note must not appear in its own completions");
    }

    #[test]
    fn dir_completion_parent_dir_option() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        idx.seed(note("/vault/sub/a.md", "[link]("));
        let params = make_completion_params("/vault/sub/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(
            items.iter().any(|i| {
                i.kind == Some(CompletionItemKind::FOLDER)
                    && text_edit_new_text(i) == Some("../")
            }),
            "should offer `../` folder item when files exist above"
        );
    }

    #[test]
    fn dir_completion_subdir_shows_children() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/subdir/b.md", ""));
        // content has the partial path already typed
        idx.seed(note("/vault/a.md", "[link](subdir/"));
        let params = make_completion_params("/vault/a.md", 0, 14);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(
            items.iter().any(|i| {
                i.kind == Some(CompletionItemKind::FILE)
                    && text_edit_new_text(i) == Some("subdir/b.md")
            }),
            "drilling into subdir/ should show its children"
        );
        assert!(!items.iter().any(|i| i.kind == Some(CompletionItemKind::FOLDER)));
    }

    #[test]
    fn dir_completion_text_edit_replaces_partial() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/subdir/b.md", ""));
        idx.seed(note("/vault/a.md", "[link](subdir/"));
        let params = make_completion_params("/vault/a.md", 0, 14);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        let item = items
            .iter()
            .find(|i| text_edit_new_text(i) == Some("subdir/b.md"))
            .expect("expected subdir/b.md item");
        let edit = match item.text_edit.as_ref().unwrap() {
            CompletionTextEdit::Edit(te) => te,
            _ => panic!("expected Edit variant"),
        };
        // range starts right after `](` (character 7) and ends at cursor (14)
        assert_eq!(edit.range.start.character, 7, "range should start right after ](");
        assert_eq!(edit.range.end.character, 14, "range should end at cursor");
        assert_eq!(edit.new_text, "subdir/b.md");
    }

    #[test]
    fn dir_completion_title_as_label() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        let item = items
            .iter()
            .find(|i| text_edit_new_text(i) == Some("b.md"))
            .expect("expected b.md item");
        assert_eq!(item.label, "My Note");
        assert_eq!(item.filter_text.as_deref(), Some("b.md"));
        assert_eq!(item.detail.as_deref(), Some("b.md"));
    }

    #[test]
    fn dir_completion_attachment_filename_label() {
        let mut idx = NoteIndex::default();
        let _ = idx.add_attachment(std::path::PathBuf::from("/vault/img.png"));
        idx.seed(note("/vault/a.md", "[link]("));
        let params = make_completion_params("/vault/a.md", 0, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        let item = items
            .iter()
            .find(|i| text_edit_new_text(i) == Some("img.png"))
            .expect("expected img.png item");
        assert_eq!(item.label, "img.png");
        assert_eq!(item.filter_text.as_deref(), Some("img.png"));
        assert!(item.detail.is_none());
    }

    // ── check_tag_trigger ─────────────────────────────────────────────────────

    #[test]
    fn tag_trigger_bare_scalar() {
        let content = "---\ntags: par\n---\n";
        // cursor after "par" (line 1, character 10)
        let result = check_tag_trigger(content, Position { line: 1, character: 10 });
        assert!(result.is_some());
        let (partial, range) = result.unwrap();
        assert_eq!(partial, "par");
        assert_eq!(range.start, Position { line: 1, character: 6 });
    }

    #[test]
    fn tag_trigger_bare_scalar_empty() {
        let content = "---\ntags: \n---\n";
        // cursor right after the space (character 6)
        let result = check_tag_trigger(content, Position { line: 1, character: 6 });
        assert!(result.is_some());
        let (partial, _) = result.unwrap();
        assert_eq!(partial, "");
    }

    #[test]
    fn tag_trigger_inline_list_partial() {
        let content = "---\ntags: [rust, we\n---\n";
        // cursor after "we" (character 17); "tags: [rust, we" → w is at byte 13
        let result = check_tag_trigger(content, Position { line: 1, character: 17 });
        assert!(result.is_some());
        let (partial, range) = result.unwrap();
        assert_eq!(partial, "we");
        assert_eq!(range.start.character, 13); // after "[rust, "
    }

    #[test]
    fn tag_trigger_inline_list_first_item() {
        let content = "---\ntags: [ru\n---\n";
        // cursor after "ru" (character 10)
        let result = check_tag_trigger(content, Position { line: 1, character: 10 });
        assert!(result.is_some());
        let (partial, _) = result.unwrap();
        assert_eq!(partial, "ru");
    }

    #[test]
    fn tag_trigger_inline_list_past_bracket_returns_none() {
        // cursor is past the closing `]`
        let content = "---\ntags: [rust]\n---\n";
        let result = check_tag_trigger(content, Position { line: 1, character: 15 });
        assert!(result.is_none());
    }

    #[test]
    fn tag_trigger_block_list_item() {
        let content = "---\ntags:\n  - rus\n---\n";
        // cursor after "rus" on line 2 (character 7)
        let result = check_tag_trigger(content, Position { line: 2, character: 7 });
        assert!(result.is_some());
        let (partial, range) = result.unwrap();
        assert_eq!(partial, "rus");
        assert_eq!(range.start.character, 4); // after "  - "
    }

    #[test]
    fn tag_trigger_block_list_second_item() {
        let content = "---\ntags:\n  - rust\n  - we\n---\n";
        // cursor after "we" on line 3 (character 6)
        let result = check_tag_trigger(content, Position { line: 3, character: 6 });
        assert!(result.is_some());
        let (partial, _) = result.unwrap();
        assert_eq!(partial, "we");
    }

    #[test]
    fn tag_trigger_block_list_wrong_key_returns_none() {
        let content = "---\ncategories:\n  - foo\n---\n";
        // cursor on line 2 under `categories:`, not `tags:`
        let result = check_tag_trigger(content, Position { line: 2, character: 7 });
        assert!(result.is_none());
    }

    #[test]
    fn tag_trigger_outside_frontmatter_returns_none() {
        let content = "---\ntags: rust\n---\ntags: body\n";
        // cursor on line 3 (body), not in frontmatter
        let result = check_tag_trigger(content, Position { line: 3, character: 10 });
        assert!(result.is_none());
    }

    #[test]
    fn tag_trigger_no_frontmatter_returns_none() {
        let content = "tags: rust\n";
        let result = check_tag_trigger(content, Position { line: 0, character: 10 });
        assert!(result.is_none());
    }

    #[test]
    fn tag_trigger_on_closing_marker_returns_none() {
        let content = "---\ntags: rust\n---\n";
        // cursor on closing `---` line (line 2)
        let result = check_tag_trigger(content, Position { line: 2, character: 2 });
        assert!(result.is_none());
    }

    // ── tag completion (US-14) ────────────────────────────────────────────────

    #[test]
    fn tag_completion_bare_scalar_returns_tags() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        // cursor is in a.md at `tags: ` position
        idx.seed(note("/vault/a.md", "---\ntags: \n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 6);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.iter().any(|i| i.label == "rust"));
    }

    #[test]
    fn tag_completion_excludes_already_used_tags() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: [rust, web]\n---\n"));
        // a.md already has "rust" — should not appear again
        idx.seed(note("/vault/a.md", "---\ntags: [rust, \n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 15);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(!items.iter().any(|i| i.label == "rust"), "rust already used, must be excluded");
        assert!(items.iter().any(|i| i.label == "web"), "web should appear");
    }

    #[test]
    fn tag_completion_filters_by_partial() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: [rust, web, review]\n---\n"));
        // cursor in a.md after "re" — only "review" should match
        idx.seed(note("/vault/a.md", "---\ntags: re\n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 8);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "review");
    }

    #[test]
    fn tag_completion_item_kind_is_value() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        idx.seed(note("/vault/a.md", "---\ntags: \n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 6);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::VALUE)));
    }

    #[test]
    fn tag_completion_text_edit_replaces_partial() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        // a.md has "ru" typed — replace range should cover "ru"
        idx.seed(note("/vault/a.md", "---\ntags: ru\n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 8);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        let item = items.iter().find(|i| i.label == "rust").unwrap();
        let edit = match item.text_edit.as_ref().unwrap() {
            CompletionTextEdit::Edit(te) => te,
            _ => panic!("expected Edit"),
        };
        assert_eq!(edit.range.start.character, 6, "replace should start after 'tags: '");
        assert_eq!(edit.range.end.character, 8, "replace should end at cursor");
        assert_eq!(edit.new_text, "rust");
    }

    #[test]
    fn tag_completion_does_not_fire_outside_frontmatter() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        // tags: appears in the body, not frontmatter
        idx.seed(note("/vault/a.md", "prose\ntags: \n"));
        let params = make_completion_params("/vault/a.md", 1, 6);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        // Should return path-completion items (no trigger context), not tag items
        assert!(!items.iter().any(|i| i.kind == Some(CompletionItemKind::VALUE)));
    }

    #[test]
    fn tag_completion_block_list_item() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        idx.seed(note("/vault/a.md", "---\ntags:\n  - \n---\n"));
        // cursor after "  - " on line 2 (character 4)
        let params = make_completion_params("/vault/a.md", 2, 4);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.iter().any(|i| i.label == "rust"));
    }

    // ── tag find-references (US-15) ───────────────────────────────────────────

    #[test]
    fn references_on_tag_returns_all_files_using_it() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: rust\n---\n"));
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        idx.seed(note("/vault/c.md", "---\ntags: web\n---\n"));
        // cursor on "rust" in a.md — tag is on line 1, character 6
        let params = make_references_params("/vault/a.md", 1, 6);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 2);
        let uris: Vec<_> = locs.iter().map(|l| l.uri.as_str()).collect();
        assert!(uris.iter().any(|u| u.ends_with("a.md")));
        assert!(uris.iter().any(|u| u.ends_with("b.md")));
        assert!(!uris.iter().any(|u| u.ends_with("c.md")));
    }

    #[test]
    fn references_on_tag_single_file() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: unique\n---\n"));
        let params = make_references_params("/vault/a.md", 1, 6);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("a.md"));
    }

    #[test]
    fn references_on_tag_location_points_to_tag_range() {
        let mut idx = NoteIndex::default();
        let content = "---\ntags: rust\n---\n";
        let parsed = crate::test_helpers::note("/vault/a.md", content);
        let tag_range = parsed.frontmatter.as_ref().unwrap().tags[0].range;
        idx.seed(parsed);
        let params = make_references_params("/vault/a.md", 1, 6);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].range, tag_range);
    }

    // ── tag go-to-definition (US-13) ──────────────────────────────────────────

    #[test]
    fn definition_on_tag_returns_all_files() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: rust\n---\n"));
        idx.seed(note("/vault/b.md", "---\ntags: rust\n---\n"));
        let params = make_definition_params("/vault/a.md", 1, 6);
        let resp = handle_definition(params, &idx);
        match resp.expect("expected Some") {
            GotoDefinitionResponse::Array(locs) => {
                assert_eq!(locs.len(), 2);
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn definition_on_tag_not_in_frontmatter_still_resolves_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", ""));
        // cursor is on a link, not a tag
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let params = make_definition_params("/vault/a.md", 0, 3);
        let resp = handle_definition(params, &idx);
        match resp.expect("expected Some") {
            GotoDefinitionResponse::Scalar(loc) => {
                assert!(loc.uri.as_str().ends_with("b.md"));
            }
            other => panic!("expected Scalar, got {:?}", other),
        }
    }

    // ── workspace symbols include tags (issue #50) ────────────────────────────

    #[test]
    fn workspace_symbols_includes_tags() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: rust\n---\n# Heading\n"));
        let params = make_workspace_symbol_params("");
        let syms = handle_workspace_symbols(params, &idx);
        assert!(syms.iter().any(|s| s.name == "rust" && s.kind == SymbolKind::KEY));
        assert!(syms.iter().any(|s| s.name == "Heading" && s.kind == SymbolKind::STRING));
    }

    #[test]
    fn workspace_symbols_tags_query_filters() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: [rust, web]\n---\n"));
        let params = make_workspace_symbol_params("ru");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "rust");
        assert_eq!(syms[0].kind, SymbolKind::KEY);
    }

    #[test]
    fn workspace_symbols_tag_container_is_filename() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/notes/my-note.md", "---\ntags: rust\n---\n"));
        let params = make_workspace_symbol_params("rust");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].container_name.as_deref(), Some("my-note.md"));
    }

    #[test]
    fn workspace_symbols_tag_location_points_to_tag_range() {
        let content = "---\ntags: rust\n---\n";
        let parsed = crate::test_helpers::note("/vault/a.md", content);
        let tag_range = parsed.frontmatter.as_ref().unwrap().tags[0].range;
        let mut idx = NoteIndex::default();
        idx.seed(parsed);
        let params = make_workspace_symbol_params("rust");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].location.range, tag_range);
    }

    #[test]
    fn workspace_symbols_tag_query_case_insensitive() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntags: Rust\n---\n"));
        let params = make_workspace_symbol_params("ru");
        let syms = handle_workspace_symbols(params, &idx);
        assert_eq!(syms.len(), 1, "lowercase query should match mixed-case tag");
        assert_eq!(syms[0].name, "Rust");
    }

    #[test]
    fn workspace_symbols_no_tags_in_note_omitted() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Heading\n"));
        let params = make_workspace_symbol_params("");
        let syms = handle_workspace_symbols(params, &idx);
        assert!(syms.iter().all(|s| s.kind != SymbolKind::KEY));
    }

    // ── Code Lens ─────────────────────────────────────────────────────────────

    fn make_code_lens_params(path: &str) -> CodeLensParams {
        CodeLensParams {
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    #[test]
    fn code_lens_no_backlinks() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "# Orphan\n"));
        let lenses = handle_code_lens(make_code_lens_params("/vault/a.md"), &idx);
        assert!(lenses.is_empty(), "orphan note should produce no lens");
    }

    #[test]
    fn code_lens_unknown_file() {
        let idx = NoteIndex::default();
        let lenses = handle_code_lens(make_code_lens_params("/vault/unknown.md"), &idx);
        assert!(lenses.is_empty(), "unknown URI should produce no lens");
    }

    #[test]
    fn code_lens_single_backlink() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Target\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)\n"));
        let lenses = handle_code_lens(make_code_lens_params("/vault/b.md"), &idx);
        assert_eq!(lenses.len(), 1);
        let cmd = lenses[0].command.as_ref().unwrap();
        assert_eq!(cmd.title, "↑ 1 backlink");
        let args = cmd.arguments.as_ref().unwrap();
        let locations: Vec<Location> = serde_json::from_value(args[2].clone()).unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri, file_uri("/vault/a.md"));
    }

    #[test]
    fn code_lens_multiple_backlinks() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/target.md", "# Target\n"));
        idx.seed(note("/vault/a.md", "[link](target.md)\n"));
        idx.seed(note("/vault/b.md", "[link](target.md)\n"));
        idx.seed(note("/vault/c.md", "[link](target.md)\n"));
        let lenses = handle_code_lens(make_code_lens_params("/vault/target.md"), &idx);
        assert_eq!(lenses.len(), 1);
        let cmd = lenses[0].command.as_ref().unwrap();
        assert_eq!(cmd.title, "↑ 3 backlinks");
        let args = cmd.arguments.as_ref().unwrap();
        let locations: Vec<Location> = serde_json::from_value(args[2].clone()).unwrap();
        assert_eq!(locations.len(), 3);
    }

    #[test]
    fn code_lens_range_is_line_zero() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Target\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)\n"));
        let lenses = handle_code_lens(make_code_lens_params("/vault/b.md"), &idx);
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].range.start, Position { line: 0, character: 0 });
        assert_eq!(lenses[0].range.end, Position { line: 0, character: 0 });
    }

    #[test]
    fn code_lens_command_name() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "# Target\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)\n"));
        let lenses = handle_code_lens(make_code_lens_params("/vault/b.md"), &idx);
        let cmd = lenses[0].command.as_ref().unwrap();
        assert_eq!(cmd.command, "editor.action.showReferences");
    }

    // ── same-file anchor completions (US-51) ──────────────────────────────────

    #[test]
    fn completion_bare_anchor_returns_current_file_headings() {
        let mut idx = NoteIndex::default();
        // line 0: "## Alpha", line 1: "## Beta", line 2: "", line 3: "[see](#"
        idx.seed(note("/vault/a.md", "## Alpha\n## Beta\n\n[see](#"));
        let params = make_completion_params("/vault/a.md", 3, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert_eq!(items.len(), 2);
        let slugs: Vec<_> = items.iter().filter_map(|i| i.insert_text.as_deref()).collect();
        assert!(slugs.contains(&"alpha"));
        assert!(slugs.contains(&"beta"));
    }

    #[test]
    fn completion_bare_anchor_empty_headings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "no headings\n\n[see](#"));
        let params = make_completion_params("/vault/a.md", 2, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty());
    }

    #[test]
    fn completion_bare_anchor_does_not_include_other_notes() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "## Other Note Heading\n"));
        idx.seed(note("/vault/a.md", "## My Heading\n\n[see](#"));
        let params = make_completion_params("/vault/a.md", 2, 7);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "My Heading");
    }

    // ── same-file anchor definition (US-48) ───────────────────────────────────

    #[test]
    fn definition_same_file_anchor_navigates_to_heading() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n\n[jump](#section)"));
        // cursor on `[jump](#section)` at line 2
        let params = make_definition_params("/vault/a.md", 2, 5);
        let loc = unwrap_scalar(handle_definition(params, &idx));
        assert!(loc.uri.as_str().ends_with("a.md"));
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn definition_same_file_anchor_missing_falls_back_to_top() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n\n[jump](#missing)"));
        let params = make_definition_params("/vault/a.md", 2, 5);
        let loc = unwrap_scalar(handle_definition(params, &idx));
        assert!(loc.uri.as_str().ends_with("a.md"));
        assert_eq!(loc.range, Range::default());
    }

    // ── bare anchor diagnostics (US-50) ───────────────────────────────────────

    #[test]
    fn diagnostics_bare_anchor_valid() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Existing\n\n[text](#existing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_bare_anchor_broken() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Existing\n\n[text](#missing)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("#missing"));
    }

    #[test]
    fn diagnostics_bare_anchor_no_headings() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](#anything)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn diagnostics_bare_anchor_empty_slug_no_diagnostic() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[text](#)"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty());
    }

    // ── find references on heading (US-49) ────────────────────────────────────

    #[test]
    fn references_heading_includes_same_file_bare_anchors() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n\n[link](#section)"));
        // cursor on the heading line
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("a.md"));
    }

    #[test]
    fn references_heading_includes_cross_file_anchors() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n"));
        idx.seed(note("/vault/b.md", "[text](a.md#section)"));
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("b.md"));
    }

    #[test]
    fn references_heading_excludes_non_matching_anchors() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n"));
        idx.seed(note("/vault/b.md", "[text](a.md#other)"));
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert!(locs.is_empty());
    }

    #[test]
    fn references_heading_no_refs_returns_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "## Section\n"));
        let params = make_references_params("/vault/a.md", 0, 3);
        let locs = handle_references(params, &idx);
        assert!(locs.is_empty());
    }

    // ── check_frontmatter_value_trigger ───────────────────────────────────────

    #[test]
    fn check_frontmatter_value_trigger_basic() {
        let content = "---\nstatus: dr\n---\n";
        // "status: dr" — "dr" starts at byte 8, cursor after "dr" = character 10
        let result = check_frontmatter_value_trigger(content, Position { line: 1, character: 10 });
        assert!(result.is_some());
        let (key, partial, _range) = result.unwrap();
        assert_eq!(key, "status");
        assert_eq!(partial, "dr");
    }

    #[test]
    fn check_frontmatter_value_trigger_empty_partial() {
        let content = "---\nstatus: \n---\n";
        // "status: " — cursor right after the space, character 8
        let result = check_frontmatter_value_trigger(content, Position { line: 1, character: 8 });
        assert!(result.is_some());
        let (key, partial, _range) = result.unwrap();
        assert_eq!(key, "status");
        assert_eq!(partial, "");
    }

    #[test]
    fn check_frontmatter_value_trigger_before_colon() {
        let content = "---\nstatus: draft\n---\n";
        // cursor at character 3 — before the colon at position 6
        let result = check_frontmatter_value_trigger(content, Position { line: 1, character: 3 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_value_trigger_no_frontmatter() {
        let content = "status: draft\n";
        let result = check_frontmatter_value_trigger(content, Position { line: 0, character: 13 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_value_trigger_outside_block() {
        let content = "---\nstatus: ok\n---\nstatus: draft\n";
        // cursor on line 3, which is after the closing ---
        let result = check_frontmatter_value_trigger(content, Position { line: 3, character: 13 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_value_trigger_tags_key() {
        let content = "---\ntags: \n---\n";
        // cursor at character 6, after "tags: "
        let result = check_frontmatter_value_trigger(content, Position { line: 1, character: 6 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_value_trigger_inline_list() {
        let content = "---\nstatus: [a, b]\n---\n";
        // cursor inside the brackets at character 12
        let result = check_frontmatter_value_trigger(content, Position { line: 1, character: 12 });
        assert!(result.is_none());
    }

    // ── check_frontmatter_key_trigger ─────────────────────────────────────────

    #[test]
    fn check_frontmatter_key_trigger_basic() {
        let content = "---\nstat\n---\n";
        // cursor after "stat" at character 4
        let result = check_frontmatter_key_trigger(content, Position { line: 1, character: 4 });
        assert!(result.is_some());
        let (partial, _range) = result.unwrap();
        assert_eq!(partial, "stat");
    }

    #[test]
    fn check_frontmatter_key_trigger_blank_line() {
        let content = "---\n\n---\n";
        // cursor at line 1, character 0
        let result = check_frontmatter_key_trigger(content, Position { line: 1, character: 0 });
        assert!(result.is_some());
        let (partial, _range) = result.unwrap();
        assert_eq!(partial, "");
    }

    #[test]
    fn check_frontmatter_key_trigger_on_list_item() {
        let content = "---\n  - foo\n---\n";
        // cursor on list item at character 6
        let result = check_frontmatter_key_trigger(content, Position { line: 1, character: 6 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_key_trigger_in_value() {
        let content = "---\nstatus: dr\n---\n";
        // cursor after ": " on "status: dr" at character 10
        let result = check_frontmatter_key_trigger(content, Position { line: 1, character: 10 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_key_trigger_no_frontmatter() {
        let content = "stat\n";
        let result = check_frontmatter_key_trigger(content, Position { line: 0, character: 4 });
        assert!(result.is_none());
    }

    #[test]
    fn check_frontmatter_key_trigger_on_closing_delimiter() {
        let content = "---\nstatus: draft\n---\n";
        // cursor on the closing --- line (line 2)
        let result = check_frontmatter_key_trigger(content, Position { line: 2, character: 0 });
        assert!(result.is_none());
    }

    // ── Step 4: schema key completions ────────────────────────────────────────

    fn make_schema_config(fields: Vec<(&str, Option<Vec<&str>>)>) -> crate::server::Config {
        crate::server::Config {
            frontmatter_schema: crate::server::FrontmatterSchema {
                fields: fields
                    .into_iter()
                    .map(|(k, vs)| {
                        (
                            k.to_string(),
                            crate::server::SchemaField {
                                values: vs.map(|v| v.into_iter().map(String::from).collect()),
                                required: false,
                            },
                        )
                    })
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn schema_key_completion_offers_all_unused_keys() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\n\n---\n"));
        let config = make_schema_config(vec![
            ("status", Some(vec!["draft", "published"])),
            ("type", Some(vec!["note", "meeting"])),
        ]);
        // cursor on the blank frontmatter line (line 1, char 0)
        let params = make_completion_params("/vault/a.md", 1, 0);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::FIELD)));
        assert!(items.iter().any(|i| i.label == "status"));
        assert!(items.iter().any(|i| i.label == "type"));
    }

    #[test]
    fn schema_key_completion_excludes_used_keys() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: draft\n\n---\n"));
        let config = make_schema_config(vec![
            ("status", Some(vec!["draft"])),
            ("type", Some(vec!["note"])),
        ]);
        // cursor on the blank second frontmatter line (line 2)
        let params = make_completion_params("/vault/a.md", 2, 0);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "type");
    }

    #[test]
    fn schema_key_completion_filters_by_partial() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nsta\n---\n"));
        let config = make_schema_config(vec![
            ("status", Some(vec!["draft"])),
            ("type", Some(vec!["note"])),
        ]);
        // cursor at end of "sta" on line 1
        let params = make_completion_params("/vault/a.md", 1, 3);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "status");
    }

    #[test]
    fn schema_key_completion_insert_text_has_colon() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nsta\n---\n"));
        let config = make_schema_config(vec![("status", None)]);
        let params = make_completion_params("/vault/a.md", 1, 3);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 1);
        assert_eq!(text_edit_new_text(&items[0]), Some("status: "));
    }

    #[test]
    fn schema_key_completion_empty_schema_returns_empty() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nsta\n---\n"));
        let params = make_completion_params("/vault/a.md", 1, 3);
        let items = handle_completion(params, &idx, &crate::server::Config::default());
        assert!(items.is_empty());
    }

    // ── Step 5: schema diagnostics ───────────────────────────────────────────

    fn make_schema_with_required(key: &str, values: Option<Vec<&str>>) -> crate::server::Config {
        crate::server::Config {
            frontmatter_schema: crate::server::FrontmatterSchema {
                fields: vec![(
                    key.to_string(),
                    crate::server::SchemaField {
                        values: values.map(|v| v.into_iter().map(String::from).collect()),
                        required: true,
                    },
                )],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn schema_diag_required_key_absent() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\ntitle: x\n---\n"));
        let config = make_schema_with_required("status", None);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("status"), "message: {}", diags[0].message);
        assert_eq!(diags[0].range.start, Position { line: 0, character: 0 });
    }

    #[test]
    fn schema_diag_required_key_present_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: draft\n---\n"));
        let config = make_schema_with_required("status", None);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty());
    }

    #[test]
    fn schema_diag_value_match_is_exact_case() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: Draft\n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert_eq!(diags.len(), 1, "case mismatch should produce a warning");
        assert!(diags[0].message.contains("Draft"), "message: {}", diags[0].message);
    }

    #[test]
    fn schema_diag_exact_value_match_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: draft\n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty());
    }

    #[test]
    fn schema_diag_no_frontmatter_require_off_no_warning() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "no frontmatter here\n"));
        let config = make_schema_with_required("status", None);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty(), "require_frontmatter=false should not warn when frontmatter absent");
    }

    #[test]
    fn schema_diag_no_frontmatter_require_on_warns() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "no frontmatter here\n"));
        let config = crate::server::Config {
            frontmatter_schema: crate::server::FrontmatterSchema {
                fields: vec![(
                    "status".to_string(),
                    crate::server::SchemaField { values: None, required: true },
                )],
                require_frontmatter: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("status"), "message: {}", diags[0].message);
        assert_eq!(diags[0].range.start, Position { line: 0, character: 0 });
    }

    #[test]
    fn schema_diag_unknown_key_warn_off_no_diagnostic() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nfoobar: x\n---\n"));
        let config = make_schema_config(vec![("status", None)]);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty(), "warn_unknown_keys=false should not warn");
    }

    #[test]
    fn schema_diag_unknown_key_warn_on_warns() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nfoobar: x\n---\n"));
        let config = crate::server::Config {
            frontmatter_schema: crate::server::FrontmatterSchema {
                fields: vec![("status".to_string(), crate::server::SchemaField { values: None, required: false })],
                warn_unknown_keys: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foobar"), "message: {}", diags[0].message);
    }

    #[test]
    fn schema_diag_complex_value_skipped() {
        let mut idx = NoteIndex::default();
        // Inline list → parser stores value=None; enum check must be skipped.
        idx.seed(note("/vault/a.md", "---\nstatus: [a, b]\n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty(), "complex (list) value should not trigger enum diagnostic");
    }

    #[test]
    fn schema_diag_key_match_is_case_insensitive() {
        let mut idx = NoteIndex::default();
        // Schema key is "Status" (capital); note field key is "status" (lowercase).
        idx.seed(note("/vault/a.md", "---\nstatus: draft\n---\n"));
        let config = crate::server::Config {
            frontmatter_schema: crate::server::FrontmatterSchema {
                fields: vec![(
                    "Status".to_string(),
                    crate::server::SchemaField {
                        values: Some(vec!["draft".to_string(), "published".to_string()]),
                        required: false,
                    },
                )],
                ..Default::default()
            },
            ..Default::default()
        };
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &config);
        assert!(diags.is_empty(), "key lookup must be case-insensitive");
    }

    #[test]
    fn schema_empty_no_diagnostics() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\narbitrary: value\nother: thing\n---\n"));
        let diags = compute_diagnostics(Path::new("/vault/a.md"), &idx, &crate::server::Config::default());
        assert!(diags.is_empty(), "empty schema should produce no diagnostics");
    }

    // ── Step 4: schema value completions ──────────────────────────────────────

    #[test]
    fn schema_value_completion_offers_enum_values() {
        let mut idx = NoteIndex::default();
        // "status: " — cursor at char 8 (after the space)
        idx.seed(note("/vault/a.md", "---\nstatus: \n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        let params = make_completion_params("/vault/a.md", 1, 8);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::VALUE)));
        assert!(items.iter().any(|i| i.label == "draft"));
        assert!(items.iter().any(|i| i.label == "published"));
    }

    #[test]
    fn schema_value_completion_filters_by_partial() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: pub\n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        // cursor at end of "pub" — char 11
        let params = make_completion_params("/vault/a.md", 1, 11);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "published");
    }

    #[test]
    fn schema_value_completion_partial_is_case_sensitive() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: Pub\n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        // "Pub" does not match "published" (exact-case prefix)
        let params = make_completion_params("/vault/a.md", 1, 11);
        let items = handle_completion(params, &idx, &config);
        assert!(items.is_empty());
    }

    #[test]
    fn schema_value_completion_empty_partial_returns_all() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "---\nstatus: \n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft", "published"]))]);
        let params = make_completion_params("/vault/a.md", 1, 8);
        let items = handle_completion(params, &idx, &config);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn schema_value_completion_no_values_list_returns_empty() {
        let mut idx = NoteIndex::default();
        // "title: " — cursor at char 7
        idx.seed(note("/vault/a.md", "---\ntitle: \n---\n"));
        let config = make_schema_config(vec![("title", None)]);
        let params = make_completion_params("/vault/a.md", 1, 7);
        let items = handle_completion(params, &idx, &config);
        assert!(items.is_empty());
    }

    #[test]
    fn schema_value_completion_unknown_key_returns_empty() {
        let mut idx = NoteIndex::default();
        // "foobar: " — cursor at char 8
        idx.seed(note("/vault/a.md", "---\nfoobar: \n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft"]))]);
        let params = make_completion_params("/vault/a.md", 1, 8);
        let items = handle_completion(params, &idx, &config);
        assert!(items.is_empty());
    }

    #[test]
    fn schema_value_completion_tags_key_skipped() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/other.md", "---\ntags: [rust]\n---\n"));
        // "tags: " — cursor at char 6
        idx.seed(note("/vault/a.md", "---\ntags: \n---\n"));
        let config = make_schema_config(vec![("status", Some(vec!["draft"]))]);
        let params = make_completion_params("/vault/a.md", 1, 6);
        let items = handle_completion(params, &idx, &config);
        // tag trigger fires first; results are VALUE items from the tag list
        assert!(items.iter().any(|i| i.label == "rust"));
        assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::VALUE)));
    }

    // ── handle_inlay_hints ────────────────────────────────────────────────────

    fn make_inlay_hint_params(path: &str, range: Range) -> InlayHintParams {
        InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier { uri: file_uri(path) },
            range,
        }
    }

    fn full_range() -> Range {
        Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 9999, character: 9999 },
        }
    }

    #[test]
    fn inlay_hint_shows_title() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let target_range = note("/vault/a.md", "[link](b.md)").md_links[0].target_range;
        let params = make_inlay_hint_params("/vault/a.md", full_range());
        let hints = handle_inlay_hints(params, &idx);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].position, target_range.end);
    }

    #[test]
    fn inlay_hint_label_is_title_string() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let params = make_inlay_hint_params("/vault/a.md", full_range());
        let hints = handle_inlay_hints(params, &idx);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, "-> My Note"),
            _ => panic!("expected InlayHintLabel::String"),
        }
    }

    #[test]
    fn inlay_hint_omits_note_without_title() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "no frontmatter here"));
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let params = make_inlay_hint_params("/vault/a.md", full_range());
        let hints = handle_inlay_hints(params, &idx);
        assert!(hints.is_empty(), "note without title should produce no hints");
    }

    #[test]
    fn inlay_hint_omits_broken_link() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](missing.md)"));
        let params = make_inlay_hint_params("/vault/a.md", full_range());
        let hints = handle_inlay_hints(params, &idx);
        assert!(hints.is_empty(), "broken link should produce no hints");
    }

    #[test]
    fn inlay_hint_omits_url() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/a.md", "[link](https://example.com)"));
        let params = make_inlay_hint_params("/vault/a.md", full_range());
        let hints = handle_inlay_hints(params, &idx);
        assert!(hints.is_empty(), "external URL link should produce no hints");
    }

    #[test]
    fn inlay_hint_filtered_by_range() {
        let mut idx = NoteIndex::default();
        idx.seed(note("/vault/b.md", "---\ntitle: My Note\n---\n"));
        // The link is on line 0; request only line 1+
        idx.seed(note("/vault/a.md", "[link](b.md)"));
        let narrow_range = Range {
            start: Position { line: 1, character: 0 },
            end: Position { line: 9999, character: 9999 },
        };
        let params = make_inlay_hint_params("/vault/a.md", narrow_range);
        let hints = handle_inlay_hints(params, &idx);
        assert!(hints.is_empty(), "hint outside requested range should be excluded");
    }
}
