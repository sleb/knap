use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use lsp_types::{Position, Range as LspRange};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[cfg(test)]
mod tests;

/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub stem: String,
    pub wiki_links: Vec<WikiLink>,
    pub content: String, // raw source text, retained for trigger checking in completion
}

/// A `[[wiki-link]]` found in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLink {
    pub stem: String,
    pub range: LspRange,       // full [[...]] range, for Go to Definition
    pub inner_range: LspRange, // stem text only, for diagnostics
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

    Note { path: path.to_path_buf(), stem, wiki_links, content: content.to_string() }
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

            // Skip aliased links, heading anchors, and empty/whitespace-only content
            // (these forms are introduced in later releases).
            if !inner.trim().is_empty() && !inner.contains('|') && !inner.contains('#') {
                let inner_start = after_open;
                let inner_end = after_open + inner.len();
                let outer_start = open;
                let outer_end = after_open + close_offset + 2; // include ]]

                links.push(WikiLink {
                    stem: inner.trim().to_string(),
                    range: line_index.range(outer_start..outer_end),
                    inner_range: line_index.range(inner_start..inner_end),
                });
            }

            search_from = after_open + close_offset + 2;
        } else {
            // No closing ]] on this line — advance past the opening [[.
            search_from = after_open;
        }
    }

    links
}
