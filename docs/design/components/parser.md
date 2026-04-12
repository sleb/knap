# Parser

Parses a single Markdown file into a `Note`. Stateless and pure — given the same input it always returns the same output. Has no access to the Note Index.

---

## Dependencies

```toml
pulldown-cmark = "0.12"
```

---

## Types

```rust
/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub stem: String,           // filename without extension
    pub wiki_links: Vec<WikiLink>,
    pub content: String,        // raw source text, retained for trigger checking in completion
}

/// A [[wiki-link]] found in the file.
pub struct WikiLink {
    pub stem: String,           // target stem as written, e.g. "other-note"
    pub range: Range,           // full [[...]] range, for Go to Definition
    pub inner_range: Range,     // stem text only, for diagnostics
}
```

`Range` is `lsp_types::Range` (zero-indexed line/character positions).

---

## LineIndex

Converts byte offsets (what pulldown-cmark produces) to LSP line/character positions.

```rust
pub struct LineIndex {
    /// Byte offset of the start of each line.
    /// line_starts[0] = 0 (start of file)
    /// line_starts[n] = byte offset of line n
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
        // Binary search for the last line start <= byte_offset
        let line = self.line_starts.partition_point(|&s| s <= byte_offset) - 1;
        let character = byte_offset - self.line_starts[line];
        Position { line: line as u32, character: character as u32 }
    }

    pub fn range(&self, byte_range: std::ops::Range<usize>) -> Range {
        Range {
            start: self.position(byte_range.start),
            end:   self.position(byte_range.end),
        }
    }
}
```

`partition_point` is a stable binary search available on slices since Rust 1.52.

---

## parse()

```rust
pub fn parse(path: &Path, content: &str) -> Note {
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let line_index = LineIndex::new(content);
    let wiki_links = extract_wiki_links(content, &line_index);

    Note { path: path.to_path_buf(), stem, wiki_links, content: content.to_string() }
}
```

---

## extract_wiki_links()

Uses pulldown-cmark's offset iterator to walk the event stream. Wiki-links are extracted by scanning `Text` events; all other events are used only for context tracking.

```rust
fn extract_wiki_links(content: &str, line_index: &LineIndex) -> Vec<WikiLink> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let parser = Parser::new_ext(content, Options::empty())
        .into_offset_iter();

    let mut links = Vec::new();
    let mut in_code_block = false;

    for (event, _event_range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
            Event::End(TagEnd::CodeBlock)   => in_code_block = false,

            Event::Code(_) => {
                // inline code — skip entirely, no scanning
            }

            Event::Text(text) if !in_code_block => {
                scan_wiki_links(&text, content, &line_index, &mut links);
            }

            _ => {}
        }
    }

    links
}
```

Note: `_event_range` is the byte range of the whole event (e.g. the full paragraph). We don't use it directly — `scan_wiki_links` locates the `[[...]]` byte positions within `content` itself for precision.

---

## scan_wiki_links()

Scans a single text string for `[[stem]]` patterns and appends any found to `links`.

```rust
fn scan_wiki_links(
    text: &str,
    full_content: &str,
    line_index: &LineIndex,
    links: &mut Vec<WikiLink>,
) {
    // Find the byte offset of `text` within `full_content`.
    // pulldown-cmark text slices are substrings of the original content,
    // so pointer arithmetic gives us the offset.
    let text_start = text.as_ptr() as usize - full_content.as_ptr() as usize;

    let mut remaining = text;
    let mut cursor = text_start;

    while let Some(open) = remaining.find("[[") {
        cursor += open + 2;
        remaining = &remaining[open + 2..];

        // Find closing ]] on the same "run" — stop at newline
        let close_search = remaining.split('\n').next().unwrap_or("");
        if let Some(close) = close_search.find("]]") {
            let inner = &close_search[..close];

            // Skip aliased links and heading anchors (deferred to later releases)
            if !inner.contains('|') && !inner.contains('#') && !inner.is_empty() {
                let inner_start = cursor;
                let inner_end   = cursor + inner.len();
                let outer_start = inner_start - 2;   // include [[
                let outer_end   = inner_end + 2;     // include ]]

                links.push(WikiLink {
                    stem: inner.trim().to_string(),
                    range:       line_index.range(outer_start..outer_end),
                    inner_range: line_index.range(inner_start..inner_end),
                });
            }

            let advance = close + 2; // past ]]
            cursor += advance;
            remaining = &remaining[close + 2..];
        } else {
            // No closing ]] on this line — skip
            break;
        }
    }
}
```

**Edge cases handled:**
- `[[link]]` inside a fenced code block → skipped by `in_code_block` flag
- `` `[[link]]` `` inline code → skipped by the `Event::Code` arm
- `[[note|alias]]` → skipped (`contains('|')`)
- `[[note#heading]]` → skipped (`contains('#')`)
- `[[]]` empty → skipped (`is_empty()`)
- `[[link` unclosed → skipped (no `]]` found before newline)

**Not handled in v0.1:**
- Links spanning multiple lines (not valid Obsidian syntax anyway)
- HTML blocks containing `[[...]]`
