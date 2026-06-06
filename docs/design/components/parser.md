# Parser

Parses a single Markdown file into a `Note`. Stateless and pure — given the same
input it always returns the same output. Has no access to the Note Index.

---

## Dependencies

```toml
pulldown-cmark = "0.13"
```

---

## Types

```rust
/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub md_links: Vec<MarkdownLink>,
    pub content: String,          // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
}

/// A standard Markdown link or image found in the file.
pub struct MarkdownLink {
    pub text: String,                   // link text or image alt text
    pub target: String,                 // path relative to the current file, or URL, raw
    pub anchor: Option<String>,         // text after `#`, trimmed; None when absent or empty
    pub is_image: bool,                 // true for `![alt](url)`
    pub range: LspRange,                // full `[text](url)` or `![alt](url)` span
    pub target_range: LspRange,         // path inside `()`, excluding anchor, for rename
    pub anchor_range: Option<LspRange>, // anchor text only, for heading rename
}

/// An ATX heading found in a note file.
pub struct Heading {
    pub text: String,          // raw heading text, e.g. "My Section"
    pub level: u8,             // ATX heading level 1–6
    pub range: LspRange,       // full heading line range
    pub text_range: LspRange,  // text-only range, excluding `## ` prefix (for rename)
}

/// YAML frontmatter extracted from the top of a note file.
/// `None` when no `---…---` block is present.
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags: Vec<Tag>,
    /// All key-value pairs in document order, including `title` and `tags`.
    /// Used by schema-driven completions and diagnostics.
    pub fields: Vec<FrontmatterField>,
}

/// A single tag extracted from the `tags:` frontmatter key.
pub struct Tag {
    pub name: String,    // tag text as written (original casing)
    pub range: LspRange, // tag name's span in the full file (for cursor hit-testing)
}

/// A single key-value pair extracted from the frontmatter block.
///
/// Only scalar values are captured; complex values (block scalars, inline
/// lists, nested objects) leave `value` and `value_range` as `None`.
pub struct FrontmatterField {
    pub key: String,
    pub key_range: LspRange,
    pub value: Option<String>,
    pub value_range: Option<LspRange>,
}
```

`LspRange` is `lsp_types::Range` (zero-indexed line/character positions).

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
    /// Full source content, retained to compute UTF-16 character offsets.
    content: String,
}

impl LineIndex {
    pub fn new(content: &str) -> Self {
        let mut starts = vec![0];
        for (offset, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(offset + 1);
            }
        }
        LineIndex { line_starts: starts, content: content.to_string() }
    }

    pub fn position(&self, byte_offset: usize) -> Position {
        // Binary search for the last line start <= byte_offset
        let line = self.line_starts.partition_point(|&s| s <= byte_offset) - 1;
        let line_start = self.line_starts[line];
        // LSP requires UTF-16 code unit offsets, not byte offsets.
        let character = self.content[line_start..byte_offset]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        Position { line: line as u32, character }
    }

    pub fn range(&self, byte_range: Range<usize>) -> LspRange {
        LspRange { start: self.position(byte_range.start), end: self.position(byte_range.end) }
    }
}
```

`partition_point` is a stable binary search available on slices since Rust 1.52.

---

## parse()

```rust
pub fn parse(path: &Path, content: &str) -> Note {
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
    let (md_links, headings) = extract_body_elements(body, body_offset, &line_index);

    Note { path: path.to_path_buf(), md_links, content: content.to_string(),
           headings, frontmatter }
}
```

The `LineIndex` is built from the full file so that all byte offsets passed to
`line_index.range()` are correct even though body extraction functions receive
only the post-frontmatter slice. The `offset` parameter threads through each
extraction function to compensate.

---

## Frontmatter extraction

### frontmatter_body_offset()

Returns the byte offset at which the document body starts — the first byte after
the closing `---\n`. Returns `0` when there is no frontmatter or the opening
`---` is unclosed.

```rust
pub fn frontmatter_body_offset(content: &str) -> usize {
    if !content.starts_with("---\n") { return 0; }
    let rest = &content[4..];
    if let Some(i) = rest.find("\n---\n") {
        4 + i + 5 // "---\n"(4) + block + "\n---\n"(5)
    } else if rest.strip_suffix("\n---").is_some() {
        content.len() // entire file is frontmatter; body is empty
    } else {
        0 // malformed / unclosed block
    }
}
```

### extract_frontmatter()

Returns `None` if no valid `---…---` block is found, or `Some(Frontmatter)`
with the `title` key parsed (if present). Tags are populated separately by
`extract_tags` and merged in by `parse()`.

### extract_tags()

Supports three forms of the `tags:` key: inline list (`tags: [foo, bar]`),
block list (`tags:\n  - foo`), and bare scalar (`tags: productivity`). Returns
`vec![]` when there is no frontmatter, no `tags:` key, or the value is a block
scalar.

### extract_frontmatter_fields()

Scans the frontmatter block line-by-line. For each line of the form `key: value`:

- Scalar values (plain, single-quoted, or double-quoted) are captured with
  `key_range` and `value_range`.
- Block scalars (`|`, `>`), inline lists (`[`), and bare keys (no value after
  `:`) produce `value: None` and `value_range: None`.
- Quotes are stripped from the captured value string but not from the range
  (the range covers the inner text, not the quotes).

All keys are captured including `title` and `tags`, so schema validation can
operate uniformly over the full frontmatter.

```rust
fn extract_frontmatter_fields(
    block: &str,
    block_start: usize,
    line_index: &LineIndex,
) -> Vec<FrontmatterField>
```

---

## extract_body_elements()

A single pulldown-cmark pass over the post-frontmatter body that collects
headings and standard Markdown links/images.

pulldown-cmark parses standard Markdown links natively — no raw scanning needed.
Each `Event::Start(Tag::Link { .. })` or `Event::Start(Tag::Image { .. })` event
carries the destination URL and the byte range of the full link span.

```rust
fn extract_body_elements(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
) -> (Vec<MarkdownLink>, Vec<Heading>)
```

### Link extraction

For each link or image event, the destination is split on `#` to separate the
path from the optional heading anchor. The byte range of the full `[text](url)`
span is available from the event; `target_range` (path inside `()`) and
`anchor_range` are derived by scanning the raw source bytes within that span to
locate the `(` delimiter, the optional `#` separator, and the `)` closer.

**Edge cases handled:**

- External URLs (`https://`, `http://`, etc.) — captured as `MarkdownLink` with
  the URL as `target`; the Note Index skips resolution for external targets.
- Anchor-only links (`[text](#heading)`) — `target` is empty string; handled by
  the Note Index as a same-file anchor reference.
- Images (`![alt](path)`) — captured with `is_image: true`.
- Links inside fenced code blocks and inline code spans — pulldown-cmark excludes
  these automatically; no special handling needed.
