# Parser

Parses a single Markdown file into a `Note`. Stateless and pure — given the same
input it always returns the same output. Has no access to the Note Index.

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

Converts byte offsets (what pulldown-cmark produces) to LSP line/character
positions.

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

pulldown-cmark fragments `[[note]]` into individual character `Text` events
(`"["`, `"["`, `"note"`, `"]"`, `"]"`), so scanning within `Text` events will
never see the full `[[` sequence. Instead, the event stream is used only to
collect **exclusion zones** — byte ranges that must not be scanned — and then
the full content string is scanned directly.

```rust
fn extract_wiki_links(content: &str, line_index: &LineIndex) -> Vec<WikiLink> {
    let exclusions = collect_exclusions(content);
    scan_wiki_links(content, &exclusions, line_index)
}
```

### collect_exclusions()

Walks the pulldown-cmark event stream and records the byte ranges of fenced code
blocks and inline code spans. Everything else is fair game for scanning.

```rust
fn collect_exclusions(content: &str) -> Vec<Range<usize>> {
    let parser = Parser::new_ext(content, Options::empty()).into_offset_iter();
    let mut exclusions = Vec::new();
    let mut code_block_start: Option<usize> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => { code_block_start = Some(range.start); }
            Event::End(TagEnd::CodeBlock)   => {
                if let Some(start) = code_block_start.take() {
                    exclusions.push(start..range.end);
                }
            }
            Event::Code(_) => { exclusions.push(range); }  // inline code span
            _ => {}
        }
    }
    exclusions
}
```

### scan_wiki_links()

Scans the full content string for `[[stem]]` patterns, skipping any position
inside an exclusion zone.

```rust
fn scan_wiki_links(
    content: &str,
    exclusions: &[Range<usize>],
    line_index: &LineIndex,
) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut search_from = 0;

    while let Some(open_offset) = content[search_from..].find("[[") {
        let open = search_from + open_offset;

        if exclusions.iter().any(|ex| ex.contains(&open)) {
            search_from = open + 1;
            continue;
        }

        let after_open = open + 2;
        let line_end = content[after_open..].find('\n')
            .map(|n| after_open + n)
            .unwrap_or(content.len());
        let line_slice = &content[after_open..line_end];

        if let Some(close_offset) = line_slice.find("]]") {
            let inner = &line_slice[..close_offset];
            let close = after_open + close_offset + 2; // byte offset after ]]

            if !inner.trim().is_empty() {
                // Strip alias suffix ([[note|display]]) then anchor suffix
                // ([[note#section]]) to isolate the target note stem.
                let note_part = inner.split('|').next().unwrap_or(inner);
                let note_part = note_part.split('#').next().unwrap_or(note_part);
                let stem = note_part.trim();

                // Skip if only a `#section` or `|alias` with no note name.
                if !stem.is_empty() {
                    let leading = note_part.len() - note_part.trim_start().len();
                    let inner_start = after_open + leading;
                    let inner_end = inner_start + stem.len();

                    links.push(WikiLink {
                        stem: stem.to_string(),
                        range:       line_index.range(open..close),
                        inner_range: line_index.range(inner_start..inner_end),
                    });
                }
            }

            search_from = close;
        } else {
            search_from = after_open; // no ]] on this line, keep scanning
        }
    }
    links
}
```

**Edge cases handled:**

- `[[link]]` inside a fenced code block → excluded by `collect_exclusions`
- `` `[[link]]` `` inline code → excluded by `collect_exclusions`
- `[[note|display text]]` → alias stripped; stem is `"note"`
- `[[note#section]]` → anchor stripped; stem is `"note"`
- `[[#section]]` / `[[|alias]]` → no note name; skipped
- `[[]]` / `[[   ]]` empty/whitespace → skipped (`trim().is_empty()`)
- `[[link` unclosed → skipped (no `]]` found before newline)

`inner_range` covers the stem bytes only (after any leading whitespace, before
`|` or `#`), so diagnostic squiggles land on the note name regardless of alias
or anchor suffix.

**Not handled in v0.1:**

- Links spanning multiple lines (not valid Obsidian syntax anyway)
- HTML blocks containing `[[...]]`
