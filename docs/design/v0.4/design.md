# v0.4 Design — Hover Previews

Covers the stories in the v0.4 release:

| Story | Feature                                                           |
| ----- | ----------------------------------------------------------------- |
| US-23 | Frontmatter `title` used as display name in completions and hover |
| US-09 | Hover on `[[wiki-link]]` → preview of target note                 |
| US-10 | Hover on standard Markdown link/image → summary                   |

**Out of scope for v0.4:** frontmatter tags (v0.5), frontmatter validation
(v0.8), hover for headings, code actions (v0.6). The `hoverPreviewLines` config
knob is not added yet — use the fixed constant `PREVIEW_LINES = 10`.

---

## Frontmatter syntax

YAML frontmatter is a block delimited by `---` on the first line and a closing
`---` on its own line. Only the `title` field is consumed in v0.4:

```yaml
---
title: My Note Title
tags: [foo, bar]
---
Body content starts here.
```

Supported `title` value forms (same subset Obsidian accepts):

| Form                      | Result            |
| ------------------------- | ----------------- |
| `title: Plain title`      | `"Plain title"`   |
| `title: "Double quoted"`  | `"Double quoted"` |
| `title: 'Single quoted'`  | `"Single quoted"` |
| `title:` (empty)          | `None`            |
| `title:` absent           | `None`            |
| Block scalars (`\|`, `>`) | Treated as `None` |

Multi-line block scalars are not common for titles and parsing them fully would
require a real YAML library. Treat any value starting with `|` or `>` as absent.

---

## Data structures

### Frontmatter (new)

```rust
pub struct Frontmatter {
    pub title: Option<String>,
}
```

Populated by `extract_frontmatter()` in the parser. `None` when no frontmatter
block is found; `Some(Frontmatter { title: None })` when the block exists but
has no `title` key.

### MarkdownLink (new)

```rust
pub struct MarkdownLink {
    pub text:     String,    // link text or image alt text
    pub target:   String,    // URL or relative path, raw (unresolved)
    pub is_image: bool,      // true for `![alt](url)`
    pub range:    LspRange,  // full `[text](url)` or `![alt](url)` span
}
```

### Note (changed)

```rust
pub struct Note {
    pub path:        PathBuf,
    pub stem:        String,
    pub wiki_links:  Vec<WikiLink>,
    pub content:     String,
    pub headings:    Vec<Heading>,
    pub frontmatter: Option<Frontmatter>,  // new
    pub md_links:    Vec<MarkdownLink>,    // new
}
```

---

## Parser changes

### Frontmatter extraction

Add `extract_frontmatter(content: &str) -> Option<Frontmatter>`.

Algorithm:

1. Return `None` if `content` does not start with `---\n`.
2. Find the next `\n---\n` (or `\n---` at end-of-input). If absent, return `None`.
3. Scan lines in the block for one matching `^title:\s*(.+)`.
4. Strip surrounding double or single quotes from the captured value.
5. If the stripped value is empty, or starts with `|` or `>`, store `None` for
   `title`. Otherwise store `Some(trimmed_value)`.

This runs before the pulldown-cmark pass so frontmatter is not misinterpreted as
Markdown content. (pulldown-cmark itself does not strip frontmatter by default.)

### Standard Markdown link extraction

Extend the pulldown-cmark offset iterator pass (already used for headings) to
capture `Tag::Link` and `Tag::Image` events:

```
for (event, byte_range) in parser.into_offset_iter():
    Start(Tag::Link { dest_url, .. }) →
        current_link = (dest_url.to_string(), byte_range, text_buf="", is_image=false)
    Start(Tag::Image { dest_url, .. }) →
        current_link = (dest_url.to_string(), byte_range, text_buf="", is_image=true)
    Text(s) if inside_link →
        text_buf += s
    End(TagEnd::Link | TagEnd::Image) →
        md_links.push(MarkdownLink {
            text:     text_buf.trim().to_string(),
            target:   dest_url,
            is_image: is_image,
            range:    line_index.range(byte_range),
        })
```

Wiki-links (`[[…]]`) are plain text to pulldown-cmark and are not affected.
Inline links inside fenced code blocks are not emitted by pulldown-cmark and are
therefore not collected.

### `knap parse` CLI output

The debug CLI `knap parse <file>` should print:

- The frontmatter title (if any) alongside the stem
- Each `MarkdownLink` with text, target, and range

---

## Completion changes (US-23)

`handle_completion` currently produces items with `label = stem`. When a note
has a frontmatter title, the item should show the title to the user but still
insert the stem (because `[[title]]` would be a broken link if the filename
doesn't match).

Updated `CompletionItem` fields:

| Field         | Value                                             |
| ------------- | ------------------------------------------------- |
| `label`       | `title` if `Some`, else `stem`                    |
| `filter_text` | `stem` (so typing by filename still works)        |
| `insert_text` | `stem` (what gets inserted into the document)     |
| `detail`      | `stem` when `label` is the title (disambiguation) |
| `kind`        | `File` (unchanged)                                |

`filter_text` is always the stem. Editors use it for fuzzy matching against what
the user has typed. Setting it to the stem means typing either part of the
filename or part of the title can match, depending on how aggressively the editor
fuzzy-matches against `label` vs. `filter_text`. This is the best we can do
within standard LSP.

---

## Hover handler (US-09, US-10)

New handler `handle_hover(params: HoverParams, index: &NoteIndex) → Option<Hover>`.

```
handle_hover(params, index):
    path = uri_to_path(params.text_document.uri)
    note = index.get_note(&path)?

    // 1. Wiki-link at cursor position
    if let Some(link) = find_wiki_link_at_position(&note, params.position):
        return hover_for_wiki_link(&link, index)

    // 2. Standard Markdown link at cursor position
    if let Some(md_link) = find_md_link_at_position(&note, params.position):
        return hover_for_md_link(&md_link, index, &path)

    None
```

### Wiki-link hover (US-09)

```
hover_for_wiki_link(link, index) → Option<Hover>:
    target_path = match index.resolve(&link.stem):
        Found(p) → p
        Broken | Ambiguous → return None   // diagnostic already covers this
    target_note = index.get_note(&target_path)?
    Some(Hover {
        contents: MarkupContent { kind: Markdown, value: render_preview(target_note) },
        range:    Some(link.range),
    })
```

Broken and ambiguous links already have diagnostics; hover adds no new
information there. Return `None` so the editor shows no tooltip rather than
duplicating a message the diagnostic already displays.

### Markdown link hover (US-10)

```
hover_for_md_link(md_link, index, current_path) → Option<Hover>:
    let content = if md_link.is_image:
        format!("**Image**\n\n`{}`", md_link.target)
    else if !is_external_url(&md_link.target):
        resolved = current_path.parent().join(&md_link.target)
        if let Some(note) = index.get_note(&resolved):
            render_preview(note)                       // local note: full preview
        else:
            format!("`{}`", md_link.target)            // local non-note file: path only
    else:
        format!("[{}]({})", md_link.text, md_link.target)  // external URL

    Some(Hover {
        contents: MarkupContent { kind: Markdown, value: content },
        range:    Some(md_link.range),
    })
```

`is_external_url` returns `true` for targets starting with `http://`, `https://`,
`//`, `mailto:`, `ftp://`.

### `render_preview`

```
const PREVIEW_LINES: usize = 10;

render_preview(note: &Note) -> String:
    title = note.frontmatter.as_ref()
                .and_then(|f| f.title.as_deref())
                .unwrap_or(&note.stem)

    body  = body_after_frontmatter(&note.content)
    lines = body.lines().collect::<Vec<_>>()

    let (preview, truncated) = if lines.len() <= PREVIEW_LINES:
        (lines.join("\n"), false)
    else:
        (lines[..PREVIEW_LINES].join("\n"), true)

    let suffix = if truncated { "\n…" } else { "" }
    format!("**{title}**\n\n{preview}{suffix}")
```

`body_after_frontmatter` skips the `---…---` block if present:

```
body_after_frontmatter(content: &str) -> &str:
    if !content.starts_with("---\n") { return content }
    let rest = &content[4..]
    if let Some(i) = rest.find("\n---\n") { return &rest[i + 5..] }
    if rest.ends_with("\n---") { return "" }
    content  // malformed frontmatter; show whole file
```

---

## Capability advertisement

```rust
hover_provider: Some(HoverProviderCapability::Simple(true)),
```

No new `ServerCapabilities` fields are touched for completions — the completion
capability was already advertised in v0.1.

---

## Startup sequence (unchanged)

No changes. Frontmatter and `md_links` are populated during the initial crawl and
on each `index(note)` call, which already re-parses the file.

---

## Testing

### Unit tests

**Parser (`src/parser/tests.rs`):**

| Test                               | What it verifies                                                 |
| ---------------------------------- | ---------------------------------------------------------------- |
| `frontmatter_title_plain`          | `title: My Title` → `Frontmatter { title: Some("My Title") }`    |
| `frontmatter_title_double_quoted`  | `title: "Quoted"` → `Some("Quoted")` (quotes stripped)           |
| `frontmatter_title_single_quoted`  | `title: 'Quoted'` → `Some("Quoted")`                             |
| `frontmatter_title_absent`         | Block with no `title:` key → `Frontmatter { title: None }`       |
| `frontmatter_no_block`             | Content without `---` → `note.frontmatter == None`               |
| `frontmatter_unclosed`             | Opening `---` with no closing `---` → `note.frontmatter == None` |
| `frontmatter_block_scalar_ignored` | `title: \| …` → `title: None`                                    |
| `md_link_basic`                    | `[text](url)` → `MarkdownLink { text, target, is_image: false }` |
| `md_link_image`                    | `![alt](img.png)` → `MarkdownLink { is_image: true, … }`         |
| `md_link_range`                    | `[text](url)` at known offset → correct `range`                  |
| `md_link_in_fenced_code_ignored`   | Link inside ` ``` ` block → not in `md_links`                    |

**Handlers (`src/handlers.rs` inline tests):**

| Test                                  | What it verifies                                                            |
| ------------------------------------- | --------------------------------------------------------------------------- |
| `completion_uses_title_as_label`      | Note with `title: My Title` → item label is "My Title", insert_text is stem |
| `completion_falls_back_to_stem`       | Note with no frontmatter → label equals stem                                |
| `hover_wiki_link_resolved`            | `[[b]]`, b has title "B Note" → Hover contains "**B Note**"                 |
| `hover_wiki_link_broken_returns_none` | `[[missing]]` → `None`                                                      |
| `hover_wiki_link_shows_preview_lines` | Target has 20 lines → hover body is truncated to 10 with `…`                |
| `hover_wiki_link_skips_frontmatter`   | Target has frontmatter + body → hover shows body, not `---` block           |
| `hover_md_link_external_url`          | `[text](https://example.com)` → shows formatted link                        |
| `hover_md_link_local_note`            | `[text](./other.md)` resolves to indexed note → shows note preview          |
| `hover_md_link_image`                 | `![alt](img.png)` → shows "**Image**" header                                |
| `hover_off_link_returns_none`         | Cursor not on any link → `None`                                             |

### Integration tests

**`tests/hover.rs` (new file):**

| Test                            | What it verifies                                              |
| ------------------------------- | ------------------------------------------------------------- |
| `hover_wiki_link_round_trip`    | Hover on `[[note]]` → MarkupContent with note title + preview |
| `hover_no_link_round_trip`      | Hover on plain text → null response                           |
| `hover_md_link_round_trip`      | Hover on `[text](./other.md)` → note preview                  |
| `hover_external_url_round_trip` | Hover on `[text](https://…)` → formatted link string          |

**`tests/completion.rs` (extend existing):**

| Test                        | What it verifies                                        |
| --------------------------- | ------------------------------------------------------- |
| `completion_title_as_label` | Note with frontmatter title → title is completion label |
