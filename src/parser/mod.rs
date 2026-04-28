use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use lsp_types::{Position, Range as LspRange};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag as PdTag, TagEnd};

#[cfg(test)]
mod tests;

/// A single tag extracted from the `tags:` frontmatter key.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,    // tag text as written (original casing)
    pub range: LspRange, // tag name's span in the full file (for cursor hit-testing)
}

/// A single key-value pair extracted from the frontmatter block.
///
/// Only scalar values are captured; complex values (block scalars, inline
/// lists, nested objects) leave `value` and `value_range` as `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontmatterField {
    pub key: String,
    pub key_range: LspRange,
    pub value: Option<String>,
    pub value_range: Option<LspRange>,
}

/// YAML frontmatter extracted from the top of a note file.
/// `None` when no `---…---` block is present; `Some` when the block exists
/// (even if `title` or `tags` are absent from it).
#[derive(Debug, Clone, PartialEq)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags: Vec<Tag>,
    /// All key-value pairs in document order, including `title` and `tags`.
    pub fields: Vec<FrontmatterField>,
}

/// A standard Markdown link or image found in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownLink {
    pub text: String,    // link text or image alt text
    pub target: String,  // URL or relative path, raw (unresolved)
    pub is_image: bool,  // true for `![alt](url)`
    pub range: LspRange, // full `[text](url)` or `![alt](url)` span
}

/// The parsed representation of a single note file.
#[derive(Debug)]
pub struct Note {
    pub path: PathBuf,
    pub stem: String,
    pub wiki_links: Vec<WikiLink>,
    pub content: String, // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
    pub md_links: Vec<MarkdownLink>,
}

/// An ATX heading found in a note file.
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub text: String,       // raw heading text, e.g. "My Section"
    pub level: u8,          // ATX heading level 1–6
    pub range: LspRange,    // full heading line range (for navigation and DocumentSymbol)
    pub text_range: LspRange, // text-only range, excluding `## ` prefix (for rename)
}

impl Note {
    /// Full filename including extension (e.g. `"my-note.md"`).
    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .expect("note path must have a filename")
            .to_string_lossy()
            .into_owned()
    }
}

/// A `[[wiki-link]]` found in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLink {
    pub stem: String,
    pub anchor: Option<String>,       // text after `#`, before `|`, trimmed; None when absent or empty
    pub range: LspRange,              // full [[...]] range, for Go to Definition
    pub inner_range: LspRange,        // stem text only, for diagnostics and file rename
    pub anchor_range: Option<LspRange>, // anchor text only (for heading rename and code action)
}

/// Maps byte offsets (from pulldown-cmark) to LSP line/character positions.
pub struct LineIndex {
    /// Byte offset of the start of each line.
    /// line_starts[0] = 0, line_starts[n] = byte offset of line n.
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(content: &str) -> Self {
        let mut starts = vec![0];
        for (offset, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(offset + 1);
            }
        }
        LineIndex { line_starts: starts }
    }

    pub fn position(&self, byte_offset: usize) -> Position {
        let line = self.line_starts.partition_point(|&s| s <= byte_offset) - 1;
        let character = byte_offset - self.line_starts[line];
        Position { line: line as u32, character: character as u32 }
    }

    pub fn range(&self, byte_range: Range<usize>) -> LspRange {
        LspRange {
            start: self.position(byte_range.start),
            end: self.position(byte_range.end),
        }
    }
}

pub fn parse(path: &Path, content: &str) -> Note {
    let stem = path
        .file_stem()
        .expect("note path must have a filename")
        .to_string_lossy()
        .into_owned();

    let line_index = LineIndex::new(content); // full content — keeps LSP positions correct
    let frontmatter = extract_frontmatter(content).map(|mut fm| {
        fm.tags = extract_tags(content, &line_index);
        if let Some(block) = frontmatter_block(content) {
            fm.fields = extract_frontmatter_fields(block, 4, &line_index);
        }
        fm
    });
    let body_offset = frontmatter_body_offset(content);
    let body = &content[body_offset..];
    let (wiki_links, headings, md_links) = extract_body_elements(body, body_offset, &line_index);

    Note {
        path: path.to_path_buf(),
        stem,
        wiki_links,
        content: content.to_string(),
        headings,
        frontmatter,
        md_links,
    }
}

/// Return the block of text between the frontmatter delimiters (`---`…`---`),
/// or `None` when the content has no valid frontmatter.
fn frontmatter_block(content: &str) -> Option<&str> {
    if !content.starts_with("---\n") {
        return None;
    }
    let rest = &content[4..];
    if let Some(i) = rest.find("\n---\n") {
        Some(&rest[..i])
    } else {
        rest.strip_suffix("\n---")
    }
}

/// Extract YAML frontmatter from the start of `content`.
///
/// Returns `None` if no valid `---…---` block is found.
/// Returns `Some(Frontmatter { title: None })` if the block exists but has no
/// `title:` key (or the value is empty / a block scalar).
pub fn extract_frontmatter(content: &str) -> Option<Frontmatter> {
    let block = frontmatter_block(content)?;

    // Scan block lines for the first `title:` key.
    let mut title: Option<String> = None;
    for line in block.lines() {
        if let Some(raw) = line.strip_prefix("title:") {
            let value = raw.trim();
            if value.is_empty() || value.starts_with('|') || value.starts_with('>') {
                // empty value or block scalar — treat as absent
            } else {
                // Strip matching surrounding quotes.
                let inner = if value.len() >= 2
                    && ((value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\'')))
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                };
                let inner = inner.trim();
                if !inner.is_empty() {
                    title = Some(inner.to_string());
                }
            }
            break; // first `title:` line wins
        }
    }

    Some(Frontmatter { title, tags: vec![], fields: vec![] })
}

/// Extract all key-value pairs from the frontmatter block.
///
/// Scalar values (`key: value`, with optional surrounding quotes) are
/// captured with their ranges. Block scalars (`|`, `>`), inline lists (`[`),
/// and bare keys (no `:`) are skipped — those fields get `value: None`.
fn extract_frontmatter_fields(block: &str, block_start: usize, line_index: &LineIndex) -> Vec<FrontmatterField> {
    let mut fields = vec![];
    let mut offset = block_start;

    for line in block.lines() {
        let Some(colon_pos) = line.find(':') else {
            offset += line.len() + 1;
            continue;
        };

        let key = line[..colon_pos].trim();
        if key.is_empty() || key.starts_with('#') {
            offset += line.len() + 1;
            continue;
        }

        let key_start = offset + (line.len() - line.trim_start().len());
        let key_end = key_start + key.len();
        let key_range = line_index.range(key_start..key_end);

        let raw_value = line[colon_pos + 1..].trim();
        let (value, value_range) = if raw_value.is_empty()
            || raw_value.starts_with('|')
            || raw_value.starts_with('>')
            || raw_value.starts_with('[')
            || raw_value.starts_with('-')
        {
            (None, None)
        } else {
            // Strip matching surrounding quotes to get the canonical value.
            let inner = if raw_value.len() >= 2
                && ((raw_value.starts_with('"') && raw_value.ends_with('"'))
                    || (raw_value.starts_with('\'') && raw_value.ends_with('\'')))
            {
                &raw_value[1..raw_value.len() - 1]
            } else {
                raw_value
            };
            let inner = inner.trim();
            if inner.is_empty() {
                (None, None)
            } else {
                // Find where the raw_value starts within the line.
                let after_colon = &line[colon_pos + 1..];
                let leading_ws = after_colon.len() - after_colon.trim_start().len();
                let raw_start = offset + colon_pos + 1 + leading_ws;
                // If quoted, value range is inside the quotes.
                let (val_start, val_end) = if raw_value != inner {
                    (raw_start + 1, raw_start + 1 + inner.len())
                } else {
                    (raw_start, raw_start + inner.len())
                };
                (Some(inner.to_string()), Some(line_index.range(val_start..val_end)))
            }
        };

        fields.push(FrontmatterField { key: key.to_string(), key_range, value, value_range });
        offset += line.len() + 1;
    }

    fields
}

/// Return the byte offset at which the document body starts — i.e. the first
/// byte after the closing `---\n` of the frontmatter block.
///
/// Returns `0` when there is no frontmatter or the opening `---` is unclosed
/// (same condition under which `extract_frontmatter` returns `None`).
pub fn frontmatter_body_offset(content: &str) -> usize {
    let block = match frontmatter_block(content) {
        Some(b) => b,
        None => return 0,
    };
    let block_end = 4 + block.len(); // 4 = len("---\n")
    if block_end + 5 <= content.len() { block_end + 5 } else { content.len() } // 5 = len("\n---\n")
}

/// Extract tags from the frontmatter `tags:` key. Supports three forms:
/// - Inline list: `tags: [foo, bar]`
/// - Block list: `tags:\n  - foo\n  - bar`
/// - Bare scalar: `tags: productivity`
///
/// Returns `vec![]` when there is no frontmatter, no `tags:` key, or the value
/// is a block scalar (`|`, `>`).
fn extract_tags(content: &str, line_index: &LineIndex) -> Vec<Tag> {
    let block = match frontmatter_block(content) {
        Some(b) => b,
        None => return vec![],
    };

    let block_start = 4_usize; // byte offset of block start in full content
    let mut tags = vec![];
    let mut offset = block_start;
    let mut remaining = block;

    while !remaining.is_empty() {
        let (line, after) = match remaining.find('\n') {
            Some(nl) => (&remaining[..nl], &remaining[nl + 1..]),
            None => (remaining, ""),
        };
        let line_start = offset;

        if let Some(after_colon) = line.strip_prefix("tags:") {
            let value = after_colon.trim_start();

            if value.is_empty() {
                // Block list: scan subsequent lines for `- tag`
                let mut sub_offset = offset + line.len() + 1;
                let mut sub_rem = after;
                while !sub_rem.is_empty() {
                    let (sub_line, sub_after) = match sub_rem.find('\n') {
                        Some(nl) => (&sub_rem[..nl], &sub_rem[nl + 1..]),
                        None => (sub_rem, ""),
                    };
                    let stripped = sub_line.trim_start();
                    if let Some(after_dash) = stripped.strip_prefix('-') {
                        let tag_name = after_dash.trim();
                        if !tag_name.is_empty() {
                            let leading_ws = sub_line.len() - stripped.len();
                            let after_dash_offset = sub_offset + leading_ws + 1;
                            let tag_ws = after_dash.len() - after_dash.trim_start().len();
                            let tag_start = after_dash_offset + tag_ws;
                            tags.push(Tag {
                                name: tag_name.to_string(),
                                range: line_index.range(tag_start..tag_start + tag_name.len()),
                            });
                        }
                        sub_offset += sub_line.len() + 1;
                        sub_rem = sub_after;
                    } else {
                        break;
                    }
                }
            } else if value.starts_with('|') || value.starts_with('>') {
                // Block scalar — ignored
            } else if value.starts_with('[') {
                // Inline list: `tags: [foo, bar]`
                let leading_ws_len = after_colon.len() - after_colon.trim_start().len();
                let inner_start = line_start + "tags:".len() + leading_ws_len + 1; // +1 skips '['
                let inner = value
                    .strip_prefix('[')
                    .unwrap_or(value)
                    .trim_end_matches(']');
                let mut pos_in_inner = 0;
                for part in inner.split(',') {
                    let leading = part.len() - part.trim_start().len();
                    let tag_name = part.trim();
                    if !tag_name.is_empty() {
                        let tag_start = inner_start + pos_in_inner + leading;
                        tags.push(Tag {
                            name: tag_name.to_string(),
                            range: line_index.range(tag_start..tag_start + tag_name.len()),
                        });
                    }
                    pos_in_inner += part.len() + 1; // +1 for ','
                }
            } else {
                // Bare scalar: `tags: productivity`
                let leading_ws_len = after_colon.len() - after_colon.trim_start().len();
                let tag_start = line_start + "tags:".len() + leading_ws_len;
                tags.push(Tag {
                    name: value.to_string(),
                    range: line_index.range(tag_start..tag_start + value.len()),
                });
            }
            break; // Only the first `tags:` key is processed
        }

        offset += line.len() + 1;
        remaining = after;
    }

    tags
}

/// Parse `content` (the body slice, post-frontmatter) in a single pulldown-cmark
/// pass, collecting headings, standard Markdown links/images, and wiki-link
/// exclusion zones. Then does a raw scan for `[[wiki-links]]` outside those zones.
///
/// pulldown-cmark fragments `[[note]]` into individual character `Text` events,
/// so wiki-links can't be extracted from the event stream directly; the exclusion
/// zones are instead used to constrain a raw byte scan.
///
/// `offset` is the byte distance from the start of the full file to the start
/// of `content`; it is added to every byte position before calling
/// `line_index.range()`.
fn extract_body_elements(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
) -> (Vec<WikiLink>, Vec<Heading>, Vec<MarkdownLink>) {
    let parser = Parser::new_ext(content, Options::empty()).into_offset_iter();
    let mut exclusions: Vec<Range<usize>> = Vec::new();
    let mut headings: Vec<Heading> = Vec::new();
    let mut md_links: Vec<MarkdownLink> = Vec::new();

    let mut code_block_start: Option<usize> = None;
    // (level, heading_byte_start, accumulated_text, first_text_byte, last_text_end_byte)
    let mut current_heading: Option<(u8, usize, String, Option<usize>, usize)> = None;
    // (target, range_start_in_body, text_buf, is_image)
    let mut current_link: Option<(String, usize, String, bool)> = None;

    for (event, byte_range) in parser {
        match event {
            // ── Exclusion zones (code blocks / inline code) ───────────────────
            Event::Start(PdTag::CodeBlock(_)) => {
                code_block_start = Some(byte_range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    exclusions.push(start..byte_range.end);
                }
            }
            Event::Code(_) => {
                exclusions.push(byte_range.clone());
            }

            // ── Headings ──────────────────────────────────────────────────────
            Event::Start(PdTag::Heading { level, .. }) => {
                current_heading = Some((
                    heading_level_to_u8(level),
                    byte_range.start,
                    String::new(),
                    None,
                    byte_range.start,
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, heading_start, text, first_start, last_text_end)) =
                    current_heading.take()
                {
                    let range =
                        line_index.range((heading_start + offset)..(byte_range.end + offset));
                    let text_range = match first_start {
                        Some(ts) => line_index.range((ts + offset)..(last_text_end + offset)),
                        None => line_index
                            .range((heading_start + offset)..(heading_start + offset)),
                    };
                    headings.push(Heading {
                        text: text.trim().to_string(),
                        level,
                        range,
                        text_range,
                    });
                }
            }

            // ── Standard Markdown links and images ────────────────────────────
            Event::Start(PdTag::Link { dest_url, .. }) => {
                current_link =
                    Some((dest_url.to_string(), byte_range.start, String::new(), false));
            }
            Event::Start(PdTag::Image { dest_url, .. }) => {
                current_link =
                    Some((dest_url.to_string(), byte_range.start, String::new(), true));
            }
            Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => {
                if let Some((target, range_start, text, is_image)) = current_link.take() {
                    md_links.push(MarkdownLink {
                        text: text.trim().to_string(),
                        target,
                        is_image,
                        range: line_index
                            .range((range_start + offset)..(byte_range.end + offset)),
                    });
                }
            }

            // ── Text (accumulated by whichever collector is active) ───────────
            Event::Text(s) => {
                if let Some((_, _, ref mut text, ref mut first_start, ref mut last_end)) =
                    current_heading
                {
                    if first_start.is_none() {
                        *first_start = Some(byte_range.start);
                    }
                    *last_end = byte_range.end;
                    text.push_str(&s);
                }
                if let Some((_, _, ref mut text, _)) = current_link {
                    text.push_str(&s);
                }
            }

            _ => {}
        }
    }

    let wiki_links = scan_wiki_links(content, offset, &exclusions, line_index);
    (wiki_links, headings, md_links)
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Scan `content` (the body slice, post-frontmatter) for `[[stem]]` patterns,
/// skipping positions inside exclusion zones. `offset` is the byte distance
/// from the start of the full file to the start of `content`; it is added to
/// every byte position before calling `line_index.range()`.
fn scan_wiki_links(
    content: &str,
    offset: usize,
    exclusions: &[Range<usize>],
    line_index: &LineIndex,
) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut search_from = 0;

    while let Some(open_offset) = content[search_from..].find("[[") {
        let open = search_from + open_offset;

        // Skip if inside a code block or inline code span.
        if exclusions.iter().any(|ex| ex.contains(&open)) {
            search_from = open + 1;
            continue;
        }

        let after_open = open + 2;

        // Only look for `]]` up to the end of the current line.
        let line_end = content[after_open..]
            .find('\n')
            .map(|n| after_open + n)
            .unwrap_or(content.len());

        let line_slice = &content[after_open..line_end];

        if let Some(close_offset) = line_slice.find("]]") {
            let inner = &line_slice[..close_offset];
            let close = after_open + close_offset + 2; // byte offset after ]]

            if !inner.trim().is_empty() {
                // Strip alias suffix ([[note|display]]) to isolate the note+anchor part.
                let pipe_part = inner.split('|').next().unwrap_or(inner);

                // Split on `#` to capture the optional anchor.
                // pipe_part starts at after_open in content (it's always the prefix of inner).
                let (note_part, anchor, anchor_range) = match pipe_part.find('#') {
                    Some(hash_pos) => {
                        let stem_part = &pipe_part[..hash_pos];
                        let anchor_raw = &pipe_part[hash_pos + 1..];
                        let trimmed = anchor_raw.trim();
                        if trimmed.is_empty() {
                            (stem_part, None, None)
                        } else {
                            let leading_ws = anchor_raw.len() - anchor_raw.trim_start().len();
                            let anchor_byte_start = after_open + hash_pos + 1 + leading_ws;
                            let anchor_byte_end = anchor_byte_start + trimmed.len();
                            (
                                stem_part,
                                Some(trimmed.to_string()),
                                Some(line_index.range(
                                    (anchor_byte_start + offset)..(anchor_byte_end + offset),
                                )),
                            )
                        }
                    }
                    None => (pipe_part, None, None),
                };

                let stem = note_part.trim();

                // Skip if only a `#section` or `|alias` with no note name.
                if !stem.is_empty() {
                    let leading = note_part.len() - note_part.trim_start().len();
                    let inner_start = after_open + leading;
                    let inner_end = inner_start + stem.len();

                    links.push(WikiLink {
                        stem: stem.to_string(),
                        anchor,
                        range: line_index.range((open + offset)..(close + offset)),
                        inner_range: line_index
                            .range((inner_start + offset)..(inner_end + offset)),
                        anchor_range,
                    });
                }
            }

            search_from = close;
        } else {
            // No closing ]] on this line — advance past the opening [[.
            search_from = after_open;
        }
    }

    links
}
