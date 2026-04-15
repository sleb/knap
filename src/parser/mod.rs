use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use lsp_types::{Position, Range as LspRange};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[cfg(test)]
mod tests;

/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub stem: String,
    pub wiki_links: Vec<WikiLink>,
    pub content: String, // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
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

    let line_index = LineIndex::new(content);
    let wiki_links = extract_wiki_links(content, &line_index);
    let headings = extract_headings(content, &line_index);

    Note { path: path.to_path_buf(), stem, wiki_links, content: content.to_string(), headings }
}

fn extract_wiki_links(content: &str, line_index: &LineIndex) -> Vec<WikiLink> {
    // pulldown-cmark fragments `[[note]]` into individual character Text events,
    // so we can't scan within Text events directly. Instead we use the event
    // stream only to collect byte ranges that must be excluded from scanning
    // (fenced code blocks and inline code spans), then do a raw scan of the
    // full content string.
    let exclusions = collect_exclusions(content);
    scan_wiki_links(content, &exclusions, line_index)
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

/// Extract all ATX headings from `content` using pulldown-cmark's offset iterator.
/// Headings inside fenced code blocks are automatically excluded by the parser.
fn extract_headings(content: &str, line_index: &LineIndex) -> Vec<Heading> {
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
                    let range = line_index.range(heading_start..byte_range.end);
                    let text_range = match first_start {
                        Some(ts) => line_index.range(ts..last_text_end),
                        None => line_index.range(heading_start..heading_start),
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

/// Scan `content` for `[[stem]]` patterns, skipping any position inside an exclusion zone.
fn scan_wiki_links(
    content: &str,
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
                                Some(line_index.range(anchor_byte_start..anchor_byte_end)),
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
                        range: line_index.range(open..close),
                        inner_range: line_index.range(inner_start..inner_end),
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
