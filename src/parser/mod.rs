use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use lsp_types::{Position, Range as LspRange};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[cfg(test)]
mod tests;

/// YAML frontmatter extracted from the top of a note file.
/// `None` when no `---…---` block is present; `Some` when the block exists
/// (even if `title` is absent from it).
#[derive(Debug, Clone, PartialEq)]
pub struct Frontmatter {
    pub title: Option<String>,
}

/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub stem: String,
    pub wiki_links: Vec<WikiLink>,
    pub content: String, // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
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

    let frontmatter = extract_frontmatter(content);
    let body_offset = frontmatter_body_offset(content);
    let body = &content[body_offset..];
    let line_index = LineIndex::new(content); // full content — keeps LSP positions correct
    let wiki_links = extract_wiki_links(body, body_offset, &line_index);
    let headings = extract_headings(body, body_offset, &line_index);

    Note {
        path: path.to_path_buf(),
        stem,
        wiki_links,
        content: content.to_string(),
        headings,
        frontmatter,
    }
}

/// Extract YAML frontmatter from the start of `content`.
///
/// Returns `None` if no valid `---…---` block is found.
/// Returns `Some(Frontmatter { title: None })` if the block exists but has no
/// `title:` key (or the value is empty / a block scalar).
pub fn extract_frontmatter(content: &str) -> Option<Frontmatter> {
    if !content.starts_with("---\n") {
        return None;
    }
    let rest = &content[4..]; // content after the opening "---\n"

    // Locate the closing "---" — either followed by "\n" or at end-of-input.
    let block = if let Some(i) = rest.find("\n---\n") {
        &rest[..i]
    } else if let Some(stripped) = rest.strip_suffix("\n---") {
        stripped
    } else {
        return None; // opening "---" with no matching close
    };

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

    Some(Frontmatter { title })
}

/// Return the byte offset at which the document body starts — i.e. the first
/// byte after the closing `---\n` of the frontmatter block.
///
/// Returns `0` when there is no frontmatter or the opening `---` is unclosed
/// (same condition under which `extract_frontmatter` returns `None`).
pub fn frontmatter_body_offset(content: &str) -> usize {
    if !content.starts_with("---\n") {
        return 0;
    }
    let rest = &content[4..];
    if let Some(i) = rest.find("\n---\n") {
        4 + i + 5 // "---\n"(4) + block + "\n---\n"(5)
    } else if rest.strip_suffix("\n---").is_some() {
        content.len() // entire file is frontmatter; body is empty
    } else {
        0 // malformed / unclosed block — don't skip anything
    }
}

fn extract_wiki_links(content: &str, offset: usize, line_index: &LineIndex) -> Vec<WikiLink> {
    // pulldown-cmark fragments `[[note]]` into individual character Text events,
    // so we can't scan within Text events directly. Instead we use the event
    // stream only to collect byte ranges that must be excluded from scanning
    // (fenced code blocks and inline code spans), then do a raw scan of the
    // body slice. `content` here is already the post-frontmatter body.
    let exclusions = collect_exclusions(content);
    scan_wiki_links(content, offset, &exclusions, line_index)
}

/// Collect byte ranges that must not be scanned for wiki-links:
/// fenced code blocks and inline code spans.
fn collect_exclusions(content: &str) -> Vec<Range<usize>> {
    let parser = Parser::new_ext(content, Options::empty()).into_offset_iter();
    let mut exclusions = Vec::new();
    let mut code_block_start: Option<usize> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_start = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    exclusions.push(start..range.end);
                }
            }
            Event::Code(_) => {
                exclusions.push(range);
            }
            _ => {}
        }
    }

    exclusions
}

/// Extract all ATX headings from `content` (the body slice, post-frontmatter).
/// `offset` is added to every byte position before calling `line_index.range()`.
/// Headings inside fenced code blocks are automatically excluded by the parser.
fn extract_headings(content: &str, offset: usize, line_index: &LineIndex) -> Vec<Heading> {
    let parser = Parser::new_ext(content, Options::empty()).into_offset_iter();
    let mut headings = Vec::new();
    // (level, heading_byte_start, accumulated_text, first_text_byte, last_text_end_byte)
    let mut current: Option<(u8, usize, String, Option<usize>, usize)> = None;

    for (event, byte_range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = heading_level_to_u8(level);
                current = Some((lvl, byte_range.start, String::new(), None, byte_range.start));
            }
            Event::Text(s) => {
                if let Some((_, _, ref mut text, ref mut first_start, ref mut last_end)) =
                    current
                {
                    if first_start.is_none() {
                        *first_start = Some(byte_range.start);
                    }
                    *last_end = byte_range.end;
                    text.push_str(&s);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, heading_start, text, first_start, last_text_end)) =
                    current.take()
                {
                    let range = line_index
                        .range((heading_start + offset)..(byte_range.end + offset));
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
            _ => {}
        }
    }

    headings
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
