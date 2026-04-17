# v0.4 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                               | Status      | Notes |
| ---------------------------------- | ----------- | ----- |
| 1 — Parser: frontmatter            | Done        |       |
| 2 — Completions with title (US-23) | Done        |       |
| 3 — Parser: Markdown links         | Not started |       |
| 4 — Hover: wiki-links (US-09)      | Not started |       |
| 5 — Hover: Markdown links (US-10)  | Not started |       |

---

## Step 1 — Parser: frontmatter extraction

Add `Frontmatter { title: Option<String> }` and populate it from the YAML
frontmatter block at the top of each file. This is the foundation for US-23
(completions) and the note preview in US-09/US-10.

**Deliverables:**

- `Frontmatter { pub title: Option<String> }` added to `src/parser/mod.rs`
- `extract_frontmatter(content: &str) -> Option<Frontmatter>` — looks for
  `---\n…\n---` at the start of the file and extracts the `title:` value,
  stripping surrounding quotes; treats block scalars (`|`, `>`) and empty
  values as `None`
- `frontmatter_body_offset(content: &str) -> usize` — returns the byte offset
  where the body starts (after the closing `---\n`); returns `0` for no
  frontmatter or a malformed block
- `parse()` passes `content[body_offset..]` to pulldown-cmark and threads
  `body_offset` through `extract_headings` and `extract_wiki_links` so that
  all LSP positions remain correct relative to the full file
- `Note` gains `frontmatter: Option<Frontmatter>`
- `knap parse <file>` CLI output includes the title (if any)

**Unit tests** (`src/parser/tests.rs`):

| Test                                 | What it verifies                                                    |
| ------------------------------------ | ------------------------------------------------------------------- |
| `frontmatter_title_plain`            | `title: My Title` → `Some("My Title")`                              |
| `frontmatter_title_double_quoted`    | `title: "Quoted"` → `Some("Quoted")` (quotes stripped)              |
| `frontmatter_title_single_quoted`    | `title: 'Quoted'` → `Some("Quoted")`                                |
| `frontmatter_title_absent`           | Frontmatter block with no `title:` key → `title: None`              |
| `frontmatter_no_block`               | No leading `---` → `note.frontmatter == None`                       |
| `frontmatter_unclosed`               | Opening `---` without closing `---` → `note.frontmatter == None`    |
| `frontmatter_block_scalar_ignored`   | `title: \|` → `title: None`                                         |
| `frontmatter_wiki_links_not_scanned` | `[[link]]` inside frontmatter block → not collected in `wiki_links` |
| `frontmatter_headings_not_scanned`   | Frontmatter block → no spurious setext heading in `headings`        |
| `wiki_link_range_after_frontmatter`  | `[[note]]` on body line → range is correct relative to full file    |

> **Manual checkpoint:** `cargo run -- parse <file>` on a file with a
> `title:` frontmatter field should print the title alongside the stem. A file
> without frontmatter should behave identically to before. A file with
> frontmatter should show no spurious headings from the frontmatter block.

---

## Step 2 — Completions with title (US-23)

Update `handle_completion` to use the frontmatter title as the display label
while still inserting the stem.

**Deliverables:**

- `CompletionItem.label` → title if `Some`, else stem (unchanged)
- `CompletionItem.filter_text` → stem (always; typing by filename still works)
- `CompletionItem.insert_text` → stem (always; `[[stem]]` is what gets written)
- `CompletionItem.detail` → stem when label differs from it (disambiguation)

**Unit tests** (`src/handlers.rs` inline):

| Test                             | What it verifies                                                    |
| -------------------------------- | ------------------------------------------------------------------- |
| `completion_uses_title_as_label` | Note with `title: My Title` → label "My Title", insert_text is stem |
| `completion_falls_back_to_stem`  | Note without frontmatter → label equals stem (existing behaviour)   |

**Integration test** (extend `tests/completion.rs`):

| Test                        | What it verifies                                         |
| --------------------------- | -------------------------------------------------------- |
| `completion_title_as_label` | Round-trip: note with frontmatter title → title in label |

> **Manual checkpoint:** open a workspace where some notes have `title:` in
> their frontmatter. Trigger `[[` completion. Notes with titles show their
> human-readable title in the picker; notes without frontmatter show the stem.
> Accepting any completion inserts the stem (not the title).

---

## Step 3 — Parser: standard Markdown link extraction

Add `MarkdownLink` and collect `[text](url)` and `![alt](url)` links from the
pulldown-cmark offset iterator pass. This is the prerequisite for the US-10
hover.

**Deliverables:**

- `MarkdownLink { text: String, target: String, is_image: bool, range: LspRange }`
  added to `src/parser/mod.rs`
- `Note` gains `md_links: Vec<MarkdownLink>`
- The pulldown-cmark pass extended to collect `Tag::Link` and `Tag::Image`
  events with their byte ranges
- `knap parse <file>` CLI output includes standard links

**Unit tests** (`src/parser/tests.rs`):

| Test                             | What it verifies                                                  |
| -------------------------------- | ----------------------------------------------------------------- |
| `md_link_basic`                  | `[text](url)` → correct `text`, `target`, `is_image: false`       |
| `md_link_image`                  | `![alt](img.png)` → `is_image: true`, correct `text` and `target` |
| `md_link_range`                  | `[text](url)` at a known line/column → `range` covers full span   |
| `md_link_in_fenced_code_ignored` | Link inside ` ``` ` block → not collected                         |

> **Manual checkpoint:** `cargo run -- parse <file>` on a file containing
> `[text](url)` links and `![alt](image.png)` images should print them in the
> output. Links inside code blocks should not appear.

---

## Step 4 — Hover: wiki-links (US-09)

Implement `handle_hover` and wire it into the request dispatcher. For this step,
only wiki-link positions produce a hover; Markdown links are handled in step 5.

**Deliverables:**

- `handle_hover(params: HoverParams, index: &NoteIndex) -> Option<Hover>` in
  `src/handlers.rs`
- Cursor on a `[[wiki-link]]`:
  - Resolved link → `Hover` with `MarkupContent { kind: Markdown }` containing
    `render_preview(target_note)`
  - Broken or ambiguous link → `None`
- `render_preview(note: &Note) -> String`:
  - Heading line: `**title**` (frontmatter title, or stem as fallback)
  - Body: first `PREVIEW_LINES` (10) lines after the frontmatter block
  - Appends `…` when the body is longer than `PREVIEW_LINES`
- `ServerCapabilities.hover_provider = Some(HoverProviderCapability::Simple(true))`
- `dispatch_request` routes `textDocument/hover`

**Unit tests** (`src/handlers.rs` inline):

| Test                                  | What it verifies                                                |
| ------------------------------------- | --------------------------------------------------------------- |
| `hover_wiki_link_resolved`            | `[[b]]`, b has `title: B Note` → Hover contains "**B Note**"    |
| `hover_wiki_link_broken_returns_none` | `[[missing]]` → `None`                                          |
| `hover_wiki_link_shows_preview_lines` | Target with 20 lines → body truncated to 10 with `…`            |
| `hover_wiki_link_skips_frontmatter`   | Target with frontmatter + body → body shown, `---` block absent |
| `hover_off_link_returns_none`         | Cursor on plain text → `None`                                   |

**Integration test** (`tests/hover.rs` — new file):

| Test                         | What it verifies                                              |
| ---------------------------- | ------------------------------------------------------------- |
| `hover_wiki_link_round_trip` | Hover on `[[note]]` → MarkupContent with title + body preview |
| `hover_no_link_round_trip`   | Hover on plain text → null response                           |

> **Manual checkpoint:** hover over a `[[wiki-link]]` in the editor. A tooltip
> appears with the note's title (or stem) as a bold heading and the first few
> lines of its content. Hovering over a broken link shows no tooltip (the
> diagnostic squiggle is still there).

---

## Step 5 — Hover: Markdown links (US-10)

Extend `handle_hover` to also handle standard Markdown link and image positions.

**Deliverables:**

- `find_md_link_at_position` helper (similar to `find_wiki_link_at_position`)
- `hover_for_md_link` branch inside `handle_hover`:
  - Image (`is_image: true`) → `"**Image**\n\n\`path\`"`
  - Local relative path resolving to an indexed note → `render_preview(note)`
  - Local path not in index → ``"`path`"``
  - External URL → `"[text](url)"` formatted as Markdown
- `is_external_url(target: &str) -> bool` helper — returns `true` for `http://`,
  `https://`, `//`, `mailto:`, `ftp://` prefixes

**Unit tests** (`src/handlers.rs` inline):

| Test                         | What it verifies                                              |
| ---------------------------- | ------------------------------------------------------------- |
| `hover_md_link_external_url` | `[text](https://example.com)` → formatted `[text](url)` hover |
| `hover_md_link_local_note`   | `[text](./other.md)` resolves to note → note preview shown    |
| `hover_md_link_image`        | `![alt](img.png)` → "**Image**" header with path              |

**Integration tests** (extend `tests/hover.rs`):

| Test                            | What it verifies                                     |
| ------------------------------- | ---------------------------------------------------- |
| `hover_md_link_round_trip`      | Hover on `[text](./other.md)` → note preview         |
| `hover_external_url_round_trip` | Hover on `[text](https://…)` → formatted link string |

> **Manual checkpoint:** hover over a `[text](./other-note.md)` link — the same
> preview as a wiki-link should appear. Hover over `![alt](image.png)` — an
> "Image" tooltip with the path appears. Hover over an external URL —
> a tooltip shows the link text and URL. Plain text shows nothing.

---

## Done — v0.4 complete

At this point all three v0.4 user stories are implemented and tested:

| Story | Feature                                   | Delivered in step |
| ----- | ----------------------------------------- | ----------------- |
| US-23 | Frontmatter title in completions          | Step 2            |
| US-09 | Hover preview for `[[wiki-links]]`        | Step 4            |
| US-10 | Hover summary for standard Markdown links | Step 5            |

Final check before tagging: run `cargo test`, run
`cargo clippy -- -D warnings`, then do a manual end-to-end session — verify
completions show titles, hover works on wiki-links, Markdown links, and images,
and all v0.3 features (definition, diagnostics, symbols, rename) are unaffected.
