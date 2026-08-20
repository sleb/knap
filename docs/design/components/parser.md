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

````rust
/// The parsed representation of a single note file.
pub struct Note {
    pub path: PathBuf,
    pub md_links: Vec<MarkdownLink>,
    pub content: String,          // raw source text, retained for trigger checking in completion
    pub headings: Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,
    pub code_fences: Vec<CodeFence>,
}

/// A fenced code block found in the file body.
pub struct CodeFence {
    pub start_line: u32, // zero-based line of the opening ```
    pub end_line: u32,   // zero-based line of the closing ```
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
    /// Glob-like patterns from the `ignore-link-targets:` key, in document
    /// order, not deduplicated. Broken links whose target matches one of
    /// these are suppressed as diagnostics. (v0.20)
    pub ignore_link_targets: Vec<String>,
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
````

`LspRange` is `lsp_types::Range` (zero-indexed line/character positions).

---

## LineIndex

Converts byte offsets (what pulldown-cmark produces) to LSP line/character
positions.

```rust
pub struct LineIndex<'a> {
    /// Byte offset of the start of each line.
    /// line_starts[0] = 0 (start of file)
    /// line_starts[n] = byte offset of line n
    line_starts: Vec<usize>,
    /// Borrowed source content, used to compute UTF-16 character offsets.
    content: &'a str,
}

impl<'a> LineIndex<'a> {
    pub fn new(content: &'a str) -> Self {
        let mut starts = vec![0];
        for (offset, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(offset + 1);
            }
        }
        LineIndex { line_starts: starts, content }
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

    /// The inverse of `position`: converts an LSP `Position` back to a byte
    /// offset into `content`. Clamps to `content.len()` when `position` is
    /// past the end of the content, rather than panicking — e.g. a stale
    /// position computed before an edit that shrank the file. Added in
    /// v0.12 for the Edit Applicator (`edit::apply`), which needs to turn a
    /// handler-computed `TextEdit`'s `Position` range back into a byte range
    /// it can splice with `String::replace_range`.
    pub fn offset(&self, position: Position) -> usize {
        let Some(&line_start) = self.line_starts.get(position.line as usize) else {
            return self.content.len();
        };
        let line_end = self
            .line_starts
            .get(position.line as usize + 1)
            .map_or(self.content.len(), |&next| next);
        let line = &self.content[line_start..line_end];

        let mut units = 0u32;
        for (byte_offset, ch) in line.char_indices() {
            if units >= position.character {
                return line_start + byte_offset;
            }
            units += ch.len_utf16() as u32;
        }
        line_start + line.len()
    }
}
```

`partition_point` is a stable binary search available on slices since Rust 1.52.

Unlike `position`/`range`, `LineIndex` borrows `content` rather than owning a
copy of it — the lifetime parameter ties every `LineIndex` to the buffer it
was built from.

---

## parse()

```rust
pub fn parse(path: &Path, content: &str) -> Note {
    let line_index = LineIndex::new(content); // full content — keeps LSP positions correct
    let frontmatter = extract_frontmatter(content).map(|mut fm| {
        fm.tags = extract_tags(content, &line_index);
        fm.ignore_link_targets = extract_ignore_link_targets(content);
        if let Some(block) = frontmatter_block(content) {
            fm.fields = extract_frontmatter_fields(block, 4, &line_index);
        }
        fm
    });
    let body_offset = frontmatter_body_offset(content);
    let body = &content[body_offset..];
    let (md_links, headings, code_fences) = extract_body_elements(body, body_offset, &line_index);

    Note { path: path.to_path_buf(), md_links, content: content.to_string(),
           headings, frontmatter, code_fences }
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
    let block = match frontmatter_block(content) {
        Some(b) => b,
        None => return 0, // no frontmatter, or malformed / unclosed block
    };
    let block_end = 4 + block.len(); // "---\n"(4) + block
    if block_end + 5 <= content.len() {
        block_end + 5 // + "\n---\n"(5)
    } else {
        content.len() // entire file is frontmatter; body is empty
    }
}
```

Shares the `frontmatter_block()` helper with `extract_frontmatter()` and
`extract_tags()`, so the three agree on exactly what counts as a valid
frontmatter block.

### extract_frontmatter()

Returns `None` if no valid `---…---` block is found, or `Some(Frontmatter)`
with the `title` key parsed (if present). Tags and `ignore_link_targets` are
populated separately, by `extract_tags` and `extract_ignore_link_targets`
respectively, and merged in by `parse()`.

### extract_tags()

Supports three forms of the `tags:` key: inline list (`tags: [foo, bar]`),
block list (`tags:\n  - foo`), and bare scalar (`tags: productivity`). Returns
`vec![]` when there is no frontmatter, no `tags:` key, or the value is a block
scalar.

### extract_ignore_link_targets()

Extracts glob-like patterns from the frontmatter `ignore-link-targets:` key
(v0.20). Supports the same three forms as `extract_tags` — inline list
(`ignore-link-targets: [../a/**, ../b.md]`), block list
(`ignore-link-targets:\n  - ../a/**`), and bare scalar
(`ignore-link-targets: ../a/**`) — minus per-entry ranges, since these
patterns aren't individually hit-tested the way tags are. Returns `vec![]`
when there is no frontmatter, no `ignore-link-targets:` key, or the value is
a block scalar. Entries are not deduplicated.

```rust
fn extract_ignore_link_targets(content: &str) -> Vec<String>
```

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
headings and standard Markdown links/images, followed by a raw-text fallback
scan (`find_fallback_links()`, below) for link-shaped spans pulldown-cmark
didn't parse.

pulldown-cmark parses standard Markdown links natively for the common case —
no raw scanning needed there. Each `Event::Start(Tag::Link { .. })` or
`Event::Start(Tag::Image { .. })` event carries the destination URL and the
byte range of the full link span.

```rust
fn extract_body_elements(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
) -> (Vec<MarkdownLink>, Vec<Heading>, Vec<CodeFence>)
```

### Code fence extraction

In the same pass, the function collects fenced code blocks. It watches for
`Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))` to record `start_line`
and `Event::End(TagEnd::CodeBlock)` to record `end_line` (using
`byte_range.end` on the End event to land on the closing ` ``` ` line).
`CodeBlockKind::Indented` blocks are skipped — only fenced blocks produce a
`CodeFence`.

### Link extraction

For each link or image event, the destination is split on `#` to separate the
path from the optional heading anchor. The byte range of the full `[text](url)`
span is available from the event; `target_range` (path inside `()`) and
`anchor_range` are derived by scanning the raw source bytes within that span to
locate the `(` delimiter, the optional `#` separator, and the `)` closer. This
target/anchor split is factored into a shared `split_link_destination()`
helper, reused by the fallback scan below so both paths produce identical
`MarkdownLink` shapes.

**Edge cases handled:**

- External URLs (`https://`, `http://`, etc.) — captured as `MarkdownLink` with
  the URL as `target`; the Note Index skips resolution for external targets.
- Anchor-only links (`[text](#heading)`) — `target` is empty string; handled by
  the Note Index as a same-file anchor reference.
- Images (`![alt](path)`) — captured with `is_image: true`.
- Links inside fenced code blocks and inline code spans — pulldown-cmark excludes
  these automatically; no special handling needed.
  `find_fallback_links()` (below) separately excludes inline code spans since
  it operates on raw text pulldown-cmark hasn't already filtered.

### Fallback link scan: `find_fallback_links()`

CommonMark requires a _bare_ (unwrapped) link destination to contain no
whitespace, control characters, or parentheses — pulldown-cmark enforces this
strictly: `[text](My File)` isn't truncated at the space, it isn't parsed as
a link event **at all**, and falls through as plain `Text`. A user who types
a link to a file whose name has a space in it (without knowing to wrap it in
`<...>`) gets a link that's invisible to knap entirely — no broken-link
diagnostic, no "Create note" quick action, nothing.

`find_fallback_links()` runs once after the pulldown-cmark pass, over the raw
body text, to recover exactly these spans:

```rust
fn find_fallback_links(
    content: &str,
    offset: usize,
    line_index: &LineIndex,
    existing: &[Range<usize>],    // byte ranges pulldown-cmark already captured
    fence_lines: &[(u32, u32)],   // fenced code block line ranges to skip
    code_spans: &[Range<usize>],  // inline code span byte ranges to skip
) -> Vec<MarkdownLink>
```

`code_spans` is collected by `extract_body_elements()` alongside
`existing` — one `Range` pushed per pulldown-cmark `Event::Code(_)` — and
threaded through so a bracket-like span opening inside inline code (e.g.
`` `[text](` path)` ``) is never recovered as a broken link (v0.11.1, #63).

For each `[`/`![` in the raw text not already covered by `existing`, inside
a fenced code block, or inside a `code_spans` range, it locates the matching
`]` (no nested `[`, no newline), requires an immediately-following `(`, then
balances parentheses by hand (honoring `\`-escapes, refusing to cross a
newline) to find the matching `)`. The destination text between them is
checked against
`link_destination_needs_wrapping()` — the same predicate `escape_link_target()`
uses in `src/handlers.rs` — and only spans that need wrapping are recovered;
anything pulldown-cmark could have parsed as a bare destination is left to
the main pass, avoiding duplicates.

Recovered links get the **raw, unwrapped** target (e.g. `"My File"`, not
`"<My File>"`), matching what a valid parse would have produced — this is
what lets `handle_code_actions`'s "Create note" quick action see and fix a
hand-typed `[text](My File)` the same way it would a normal broken link.
Results are merged into `md_links` and the combined list is sorted by
position for deterministic ordering.

This scan excludes `existing` link/image spans, fenced code blocks, and
inline code spans (`code_spans`) — the three cases pulldown-cmark itself
would already have accounted for one way or another, so nothing inside them
is ever eligible for fallback recovery.
