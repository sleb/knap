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
    pub stem: String,             // filename without extension
    pub wiki_links: Vec<WikiLink>,
    pub content: String,          // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
    pub md_links: Vec<MarkdownLink>,
}

/// A `[[wiki-link]]` found in the file.
pub struct WikiLink {
    pub stem: String,
    pub anchor: Option<String>,         // text after `#`, trimmed; None when absent or empty
    pub range: LspRange,                // full [[...]] range, for Go to Definition
    pub inner_range: LspRange,          // stem text only, for diagnostics and file rename
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

/// A standard Markdown link or image found in the file.
pub struct MarkdownLink {
    pub text: String,    // link text or image alt text
    pub target: String,  // URL or relative path, raw (unresolved)
    pub is_image: bool,  // true for `![alt](url)`
    pub range: LspRange, // full `[text](url)` or `![alt](url)` span
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

    Note { path: path.to_path_buf(), stem, wiki_links, content: content.to_string(),
           headings, frontmatter, md_links }
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
headings, standard Markdown links/images, and wiki-link exclusion zones, then
runs a raw byte scan for `[[wiki-links]]` outside those zones.

pulldown-cmark fragments `[[note]]` into individual character `Text` events,
so wiki-links cannot be extracted from the event stream directly. The exclusion
zones (fenced code blocks and inline code spans) collected during the parse pass
are used to constrain the raw byte scan that follows.

```rust
fn extract_body_elements(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
) -> (Vec<WikiLink>, Vec<Heading>, Vec<MarkdownLink>)
```

The single-pass design collects exclusion zones, headings, and Markdown
links/images simultaneously. A raw scan (`scan_wiki_links`) runs afterward
with the collected exclusion zones.

### scan_wiki_links()

Scans the full body string for `[[stem]]` patterns, skipping any position inside
an exclusion zone. Handles alias (`[[note|display]]`) and anchor
(`[[note#section]]`) suffixes. Records the stem, optional anchor, full range,
inner (stem-only) range, and optional anchor range.

```rust
fn scan_wiki_links(
    content: &str,
    offset: usize,
    exclusions: &[Range<usize>],
    line_index: &LineIndex,
) -> Vec<WikiLink>
```

**Edge cases handled:**

- `[[link]]` inside a fenced code block → excluded by exclusion zone
- `` `[[link]]` `` inline code → excluded by exclusion zone
- `[[note|display text]]` → alias stripped; stem is `"note"`
- `[[note#section]]` → anchor captured; stem is `"note"`
- `[[#section]]` / `[[|alias]]` → no note name; skipped
- `[[]]` / `[[   ]]` empty/whitespace → skipped
- `[[link` unclosed → skipped (no `]]` found before newline)

`inner_range` covers the stem bytes only, so diagnostic squiggles land on the
note name regardless of alias or anchor suffix. `anchor_range` covers the anchor
text only (used by heading rename).
