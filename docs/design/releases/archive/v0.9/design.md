# v0.9 Design — Editor Experience

Covers the stories in the v0.9 release:

| Story | Feature                                                                                       |
| ----- | --------------------------------------------------------------------------------------------- |
| US-36 | Folding ranges — collapse heading sections and fenced code blocks                             |
| US-52 | Selection range — smart expand/contract: word → link → paragraph → heading section → document |
| US-53 | Inlay hints — show the `title:` frontmatter of a linked note inline next to its path          |
| US-54 | Code lens on headings — `↑ N anchor links` on headings targeted by `#slug` links              |

---

## Goal

A writer using any LSP-compatible editor can rely on knap for the visual
structure cues and navigational affordances that Markdown deserves as a
first-class language. Folding lets writers collapse long sections; selection
range powers smart expand/contract; inlay hints surface linked-note titles
without leaving the current file; and heading-level code lens completes the
backlinks picture by showing how many anchor links point at each heading. These
four stories ship together because they are all pure additions to the protocol
surface — no data model changes are required except one small parser extension
— and they share no handler state, making them safe to implement in parallel.

---

## Parser Changes

Only US-36 requires a parser change: extracting fenced code block ranges so
the folding handler can emit a fold region for each block.

### New type

````rust
/// A fenced code block found in the document body.
pub struct CodeFence {
    pub start_line: u32,  // line of the opening ``` marker
    pub end_line: u32,    // line of the closing ``` marker
}
````

### New field on `Note`

```rust
pub code_fences: Vec<CodeFence>,
```

### Extraction algorithm

In `extract_body_elements`, extend the match arm handling to also watch for
`Event::Start(PdTag::CodeBlock(CodeBlockKind::Fenced(_)))` and
`Event::End(TagEnd::CodeBlock)`. On `Start`, record `byte_range.start`; on
`End`, record `byte_range.end`. Convert byte offsets to line numbers using
`line_index.position(...)`.

```rust
Event::Start(PdTag::CodeBlock(CodeBlockKind::Fenced(_))) => {
    current_fence_start = Some(line_index.position(offset + byte_range.start).line);
}
Event::End(TagEnd::CodeBlock) => {
    if let Some(start_line) = current_fence_start.take() {
        let end_line = line_index.position(offset + byte_range.end).line;
        if end_line > start_line {
            code_fences.push(CodeFence { start_line, end_line });
        }
    }
}
```

`CodeBlockKind::Indented` blocks are skipped (no `current_fence_start` set).

Edge cases:

- Fenced block not closed before EOF → pulldown-cmark closes it at end; `end_line` equals the last body line — emit the range
- Empty fence (`immediately followed by`) → `end_line == start_line`; skip (only emit when `end_line > start_line`)
- Fenced block occupying a single non-trivial body → `start_line < end_line` → emit normally

---

## Handler Changes

All five new handlers are pure functions added to `src/handlers.rs`. None require
index mutations or config changes.

---

### `handle_folding_ranges` (`textDocument/foldingRange`)

```rust
pub(crate) fn handle_folding_ranges(
    params: FoldingRangeParams,
    index: &NoteIndex,
) -> Vec<FoldingRange>
```

Two sources of ranges; both emitted as `kind: Some(FoldingRangeKind::Region)`:

**Heading sections:** For each heading at level `L`, the region spans from its
line to the line just before the next heading at the same or greater (shallower)
level — i.e., the next heading whose level number ≤ `L` — or to the last
content line of the document if no such heading follows. An H3 folds to just
before the next H1, H2, or H3; an H2 folds to just before the next H1 or H2.

```rust
let last_line = /* last non-empty line in note.content */;
for (i, heading) in note.headings.iter().enumerate() {
    let start = heading.range.start.line;
    let end = note.headings[i + 1..]
        .iter()
        .find(|h| h.level <= heading.level)
        .map(|h| h.range.start.line.saturating_sub(1))
        .unwrap_or(last_line);
    if end > start {
        ranges.push(FoldingRange { start_line: start, end_line: end, kind: Some(FoldingRangeKind::Region), .. Default::default() });
    }
}
```

**Code fences:** Each `CodeFence` becomes one region:

```rust
for fence in &note.code_fences {
    ranges.push(FoldingRange { start_line: fence.start_line, end_line: fence.end_line, kind: Some(FoldingRangeKind::Region), ..Default::default() });
}
```

Edge cases:

- Heading with no content lines below it before the next peer (end == start) → skip (single-line sections are not foldable)
- Last heading in the document → `end = last_line`; emit if `last_line > start`
- Document with no headings → return only code-fence ranges
- Code fence in the last position of the document → emit as-is

---

### `handle_selection_range` (`textDocument/selectionRange`)

```rust
pub(crate) fn handle_selection_range(
    params: SelectionRangeParams,
    index: &NoteIndex,
) -> Vec<SelectionRange>
```

For each position in `params.positions`, builds a selection range chain from
innermost to outermost. Each level is wrapped in its parent:

```
word (optional) → link (optional) → paragraph → heading section (optional) → document
```

**Word range** — not applicable when the cursor is on whitespace or punctuation;
find the maximal run of non-whitespace, non-punctuation characters at the cursor
position. If no word is found, omit this level.

**Link range** — if the cursor falls inside a `MarkdownLink.range`, the full
link span is the next outer level.

**Paragraph range** — scan backward and forward from the cursor line to find
the nearest blank lines (or document boundaries). The paragraph is the
consecutive run of non-blank lines that contains the cursor.

**Heading section range** — if the cursor line falls within a heading section
(computed the same way as in `handle_folding_ranges`), that section is the next
outer level. If the cursor is on the heading line itself, the heading section
_is_ the innermost non-word range.

**Document range** — always `Range { start: Position { line: 0, character: 0 }, end: last_position }`.

Consecutive equal ranges are collapsed (if two levels produce the same range,
only the outer is emitted). The chain is built by nesting `parent` pointers from
innermost outward.

---

### `handle_inlay_hints` (`textDocument/inlayHint`)

```rust
pub(crate) fn handle_inlay_hints(
    params: InlayHintParams,
    index: &NoteIndex,
) -> Vec<InlayHint>
```

For each `MarkdownLink` in the note whose target resolves to a note with a
non-empty `frontmatter.title`, place an inlay hint at `link.target_range.end`:

```rust
for link in &note.md_links {
    if link.target.is_empty() || index::is_url_like(&link.target) {
        continue;
    }
    let ResolvedLink::Found(target_path) = index.resolve(&path, &link.target) else {
        continue;
    };
    let Some(title) = index.get_note(&target_path)
        .and_then(|n| n.frontmatter.as_ref())
        .and_then(|fm| fm.title.as_deref())
    else {
        continue;
    };
    hints.push(InlayHint {
        position: link.target_range.end,
        label: InlayHintLabel::String(format!("-> {title}")),
        kind: Some(InlayHintKind::TYPE),
        ..Default::default()
    });
}
```

The `params.range` field is the visible document range the client wants hints for.
Filter hints to those whose `position` falls within `params.range`.

Edge cases:

- Link with an anchor (`[text](file.md#sec)`) — hint position is `target_range.end` (before `#`); the anchor suffix is unaffected
- Note without a `title:` frontmatter key → no hint
- Broken link → no hint
- Image links (`![alt](img.png)`) → include if the target is a note with a title (unlikely, but consistent)

---

### `handle_code_lens` extended for US-54

The existing signature is unchanged. The handler currently returns at most one
lens at line 0 (backlinks). US-54 adds per-heading lenses.

```rust
pub(crate) fn handle_code_lens(params: CodeLensParams, index: &NoteIndex) -> Vec<CodeLens>
```

After the existing backlinks-lens block, iterate headings:

```rust
for heading in &note.headings {
    let heading_slug = slug(&heading.text);

    // Same-file bare anchor links targeting this heading.
    let same_file_locs: Vec<Location> = note.md_links.iter()
        .filter(|l| l.target.is_empty()
            && l.anchor.as_deref().map(|a| slug(a)).as_deref() == Some(heading_slug.as_str()))
        .map(|l| Location { uri: path_to_uri(&path), range: l.range })
        .collect();

    // Cross-file anchor links targeting this heading.
    let cross_file_locs: Vec<Location> = index.links_to(&path).iter()
        .filter(|l| l.md_link.anchor.as_deref().map(|a| slug(a)).as_deref() == Some(heading_slug.as_str()))
        .map(|l| Location { uri: path_to_uri(&l.source_path), range: l.md_link.range })
        .collect();

    let all_locs: Vec<Location> = same_file_locs.into_iter().chain(cross_file_locs).collect();
    if all_locs.is_empty() {
        continue;
    }

    let count = all_locs.len();
    let command = Command {
        title: format!("↑ {} anchor link{}", count, if count == 1 { "" } else { "s" }),
        command: "editor.action.showReferences".to_string(),
        arguments: Some(vec![
            serde_json::to_value(path_to_uri(&path)).expect("URI serializable"),
            serde_json::to_value(heading.range.start).expect("Position serializable"),
            serde_json::to_value(&all_locs).expect("Locations serializable"),
        ]),
    };
    lenses.push(CodeLens {
        range: Range { start: heading.range.start, end: heading.range.start },
        command: Some(command),
        data: None,
    });
}
```

---

## Protocol Handler Changes

Three new capabilities added to `ServerCapabilities` in `src/server/mod.rs`:

```rust
folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
inlay_hint_provider: Some(OneOf::Left(true)),
// code_lens_provider already advertised from v0.6 — no change needed
```

Three new dispatch arms in `dispatch_request`:

```rust
"textDocument/foldingRange"  => { /* FoldingRangeParams → handle_folding_ranges */ }
"textDocument/selectionRange" => { /* SelectionRangeParams → handle_selection_range */ }
"textDocument/inlayHint"     => { /* InlayHintParams → handle_inlay_hints */ }
// textDocument/codeLens already dispatched — no change needed
```

---

## Testing

### Unit tests — `src/handlers.rs` (and `src/parser/`)

| File                | Test                                        | What it verifies                                                              |
| ------------------- | ------------------------------------------- | ----------------------------------------------------------------------------- |
| `src/parser/mod.rs` | `code_fence_start_end_lines`                | fenced block start/end lines are captured correctly                           |
| `src/parser/mod.rs` | `code_fence_indented_block_skipped`         | indented code block produces no `CodeFence` entry                             |
| `src/parser/mod.rs` | `code_fence_empty_skipped`                  | back-to-back fences (empty body) produce no entry                             |
| `src/handlers.rs`   | `folding_h2_section_spans_to_next_h2`       | H2 section ends on line before the next H2                                    |
| `src/handlers.rs`   | `folding_nested_h3_ends_before_parent`      | H3 section ends before its parent H2 section ends                             |
| `src/handlers.rs`   | `folding_last_heading_spans_to_doc_end`     | last heading section extends to end of document                               |
| `src/handlers.rs`   | `folding_single_line_section_omitted`       | heading with no body (end == start) produces no range                         |
| `src/handlers.rs`   | `folding_code_fence_emitted`                | fenced code block produces one region fold                                    |
| `src/handlers.rs`   | `selection_range_word_at_cursor`            | returns word bounds at cursor position                                        |
| `src/handlers.rs`   | `selection_range_cursor_in_link`            | link range is the outer level when cursor is inside a link                    |
| `src/handlers.rs`   | `selection_range_paragraph_bounds`          | paragraph range spans from first to last non-blank line                       |
| `src/handlers.rs`   | `selection_range_section_outermost_heading` | heading section is the outer level when cursor is in its body                 |
| `src/handlers.rs`   | `selection_range_document_always_outermost` | outermost level is always the full document range                             |
| `src/handlers.rs`   | `selection_range_multiple_positions`        | returns one chain per position                                                |
| `src/handlers.rs`   | `inlay_hint_shows_title`                    | link to note with `title:` produces hint `"-> {title}"` at `target_range.end` |
| `src/handlers.rs`   | `inlay_hint_omits_no_title`                 | link to note without `title:` produces no hint                                |
| `src/handlers.rs`   | `inlay_hint_omits_broken_link`              | broken link produces no hint                                                  |
| `src/handlers.rs`   | `inlay_hint_omits_url`                      | URL link produces no hint                                                     |
| `src/handlers.rs`   | `inlay_hint_filtered_by_range`              | hints outside `params.range` are omitted                                      |
| `src/handlers.rs`   | `code_lens_heading_with_same_file_anchors`  | heading with bare anchor links shows count lens                               |
| `src/handlers.rs`   | `code_lens_heading_with_cross_file_anchors` | heading with incoming cross-file anchor links shows count lens                |
| `src/handlers.rs`   | `code_lens_heading_no_anchors_no_lens`      | heading with no anchor links produces no heading lens                         |
| `src/handlers.rs`   | `code_lens_heading_lens_at_heading_line`    | heading lens `range.start` equals heading range start                         |
| `src/handlers.rs`   | `code_lens_backlinks_lens_unchanged`        | existing backlinks lens at line 0 still present alongside heading lenses      |

### Integration tests (`tests/`)

| Test                           | What it verifies                                                 |
| ------------------------------ | ---------------------------------------------------------------- |
| `folding_ranges_round_trip`    | full session returns fold regions for headings and fenced blocks |
| `selection_range_round_trip`   | full session returns correct selection chain for a given cursor  |
| `inlay_hints_round_trip`       | full session returns hints for links whose targets have a title  |
| `code_lens_heading_round_trip` | full session returns heading lenses alongside the backlinks lens |
