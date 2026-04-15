# v0.3 Design — Heading Navigation & Anchors

Covers the stories in the v0.3 release:

| Story | Feature                                                             |
| ----- | ------------------------------------------------------------------- |
| US-06 | `[[Note#Heading]]` — Go to Definition navigates to the heading line |
| US-08 | Diagnostic when heading anchor no longer exists                     |
| US-11 | Document Symbols — jump to heading within file                      |
| US-12 | Workspace Symbols — search headings across all files                |
| US-28 | Rename a heading → all `[[Note#OldHeading]]` anchor links updated   |

**Out of scope for v0.3:** frontmatter parsing, hover previews, tag features
(deferred to v0.4/v0.5). Broken-anchor code action (US-29) is deferred to v0.6
alongside US-18.

---

## Anchor syntax

`[[Note#Heading]]` is already parsed by v0.2 — the stem `Note` is extracted and
the `#Heading` suffix is stripped. v0.3 makes the anchor half useful:

| Pattern             | v0.2 behaviour                               | v0.3 behaviour                                                 |
| ------------------- | -------------------------------------------- | -------------------------------------------------------------- |
| `[[note]]`          | resolve by stem                              | unchanged                                                      |
| `[[note#Section]]`  | resolve by stem; `#Section` silently dropped | resolve by stem; navigate to heading; broken-anchor diagnostic |
| `[[note\|alias]]`   | resolve by stem                              | unchanged                                                      |
| `[[note#H\|alias]]` | resolve by stem; `#H` dropped                | resolve by stem; navigate to heading                           |

---

## Data structures

### Heading (new)

```rust
pub struct Heading {
    pub text:       String,   // raw heading text, e.g. "My Section"
    pub level:      u8,       // ATX heading level 1–6
    pub range:      LspRange, // full heading line range (for navigation and DocumentSymbol)
    pub text_range: LspRange, // text-only range, excluding `## ` prefix (for rename)
}
```

Extracted by the parser using pulldown-cmark's `Start(Tag::Heading(_)) …
End(TagEnd::Heading(_))` events. The byte range for the heading comes from
the offset iterator's range for the `Start` event; the heading text is assembled
from `Text` events within the span. `text_range` is the sub-range covering only
the heading text — computed as the start of the first `Text` event inside the
heading through the end of the last.

### Note (changed)

```rust
pub struct Note {
    pub path:       PathBuf,
    pub stem:       String,
    pub wiki_links: Vec<WikiLink>,
    pub content:    String,
    pub headings:   Vec<Heading>,  // new
}
```

### WikiLink (changed)

```rust
pub struct WikiLink {
    pub stem:         String,
    pub anchor:       Option<String>,   // new — text after `#`, before `|`, trimmed
    pub range:        LspRange,
    pub inner_range:  LspRange,
    pub anchor_range: Option<LspRange>, // new — range of just the anchor text (for rename/code action)
}
```

`inner_range` continues to span only the stem (unchanged — file-rename
correctness is unaffected). `anchor_range` spans the anchor text after `#` —
it is `None` when there is no anchor, and is used by the heading rename handler
and (in v0.6) the broken-anchor code action.

---

## Parser changes

### Heading extraction

After extracting wiki-links, make a second pass through the pulldown-cmark event
stream to collect headings. For each `Start(Tag::Heading { level, .. })` event,
collect the byte range from the offset iterator and accumulate `Text` events
until `End(TagEnd::Heading(_))`.

```
for (event, byte_range) in parser.into_offset_iter():
    Start(Heading { level }) →
        current_heading = (level, byte_range.start, text_buf="")
    Text(s) if inside_heading →
        text_buf += s
    End(Heading) →
        headings.push(Heading {
            text:  text_buf.trim().to_string(),
            level: current_level,
            range: line_index.range(byte_range_start..byte_range.end),
        })
```

Headings inside fenced code blocks are handled automatically — pulldown-cmark
does not emit `Heading` events for content inside code blocks.

### Anchor capture in `scan_wiki_links`

The existing code splits `inner` on `|` then `#` and discards the anchor. Change
it to retain the `#` portion:

```
let pipe_part = inner.split('|').next().unwrap_or(inner);
let (note_part, anchor, anchor_range) =
    match pipe_part.splitn(2, '#').collect::<Vec<_>>()[..] {
        [stem_part, anchor_part] => {
            let trimmed = anchor_part.trim();
            let anchor_byte_start = after_open + (anchor_part.as_ptr() as usize
                                                  - inner.as_ptr() as usize)
                                    + (anchor_part.len() - anchor_part.trim_start().len());
            let anchor_byte_end = anchor_byte_start + trimmed.len();
            let range = if trimmed.is_empty() { None }
                        else { Some(line_index.range(anchor_byte_start..anchor_byte_end)) };
            (stem_part, if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }, range)
        }
        [stem_part] => (stem_part, None, None),
        _           => (pipe_part, None, None),
    };
// stem = note_part.trim() as before
// WikiLink gains anchor and anchor_range fields
```

When `anchor` is `Some("")` (e.g. `[[note#]]`), treat as `None` — no anchor
to navigate to or validate.

---

## Index changes

No structural changes to `NoteIndex` in v0.3. Headings live on `Note`, which is
already stored in `by_path`. Handlers access them via `index.get_note(path)`.

---

## LSP handlers

### Go to Definition (`textDocument/definition`) — updated (US-06)

`handle_definition` already resolves the link stem. Add anchor navigation:

```
handle_definition(params, index) → Option<Location>:
    link = find_link_at_position(note, pos)?
    target_path = index.resolve(&link.stem) → Found(path) | else None

    if link.anchor is Some(anchor):
        target_note = index.get_note(&target_path)?
        heading = target_note.headings.iter()
            .find(|h| h.text.to_lowercase().trim() == anchor.to_lowercase().trim())
        if heading is Some(h):
            return Location { uri: path_to_uri(target_path), range: h.range }
        // anchor not found → fall through to file top (same as no anchor)

    return Location { uri: path_to_uri(target_path), range: Range::default() }
```

Anchor matching is **case-insensitive**, matching trimmed text. This aligns
with Obsidian and GitHub Markdown conventions.

### Diagnostics (`textDocument/publishDiagnostics`) — updated (US-08)

`compute_diagnostics` already emits `Broken` and `Ambiguous` diagnostics. Add
a third case: broken anchor.

```
for each link in note.wiki_links:
    match index.resolve(&link.stem):
        Broken    → emit "Link target not found" (unchanged)
        Ambiguous → emit "matches multiple files" (unchanged)
        Found(target_path):
            if link.anchor is Some(anchor):
                target_note = index.get_note(&target_path)
                if target_note is None or no heading matches anchor (case-insensitive):
                    emit Warning at link.inner_range:
                        "Heading not found: '#anchor' in '[[stem]]'"
```

The diagnostic range is `link.inner_range` (the stem range) — extending
`inner_range` to cover the anchor is a future polish item.

**Diagnostic message:**

```
Heading not found: '#My Section' in '[[note#My Section]]'
```

### Document Symbols (`textDocument/documentSymbol`) — new (US-11)

```
handle_document_symbols(params, index) → Vec<DocumentSymbol>:
    path = uri_to_path(params.text_document.uri)
    note = index.get_note(&path)? else []
    note.headings.iter().map(|h| DocumentSymbol {
        name:             h.text.clone(),
        kind:             SymbolKind::STRING,
        range:            h.range,
        selection_range:  h.range,
        detail:           None,
        tags:             None,
        deprecated:       None,
        children:         None,
    })
```

Return type is `DocumentSymbolResponse::Nested(Vec<DocumentSymbol>)` —
`DocumentSymbol` is preferred over the deprecated `SymbolInformation` because
it does not require a `Location` (no URI needed; the client already knows which
file it's in).

The list is flat in v0.3. Hierarchical nesting (h1 contains h2, etc.) is
deferred — it requires tracking open heading levels during the map pass and adds
complexity for marginal editor benefit.

Advertise in `ServerCapabilities`:

```rust
document_symbol_provider: Some(OneOf::Left(true)),
```

### Workspace Symbols (`workspace/symbol`) — new (US-12)

```
handle_workspace_symbols(params, index) → Vec<SymbolInformation>:
    query = params.query.to_lowercase()
    for each note in index.all_notes():
        for each heading in note.headings:
            if query is empty OR heading.text.to_lowercase().contains(&query):
                yield SymbolInformation {
                    name:           heading.text.clone(),
                    kind:           SymbolKind::STRING,
                    location:       Location { uri: path_to_uri(note.path), range: heading.range },
                    container_name: Some(note.stem.clone()),
                    tags:           None,
                    deprecated:     None,
                }
```

An empty query returns all headings — consistent with how workspace symbol
providers typically work.

Advertise in `ServerCapabilities`:

```rust
workspace_symbol_provider: Some(OneOf::Left(true)),
```

### Rename heading (`textDocument/rename` + `textDocument/prepareRename`) — new (US-28)

This is the heading-rename counterpart to US-04 (file rename). The user places
the cursor on a heading line, invokes rename (F2 / `<leader>rn`), types the new
heading text, and the server rewrites the heading in place and updates every
`[[note#OldText]]` anchor link across the workspace.

**`textDocument/prepareRename`** (optional, improves UX):

```
handle_prepare_rename(params, index) → Option<PrepareRenameResponse>:
    path = uri_to_path(params.text_document.uri)
    note = index.get_note(&path)?
    heading = note.headings.iter()
        .find(|h| h.range contains params.position)?
    return PrepareRenameResponse::RangeWithPlaceholder {
        range:       heading.text_range,
        placeholder: heading.text.clone(),
    }
```

The editor pre-fills the rename input with the current heading text and
constrains the selection to `text_range` (not the `##` prefix).

**`textDocument/rename`**:

```
handle_rename(params, index) → Option<WorkspaceEdit>:
    path = uri_to_path(params.text_document.uri)
    note = index.get_note(&path)?
    heading = note.headings.iter()
        .find(|h| h.range contains params.position)?

    old_text = heading.text.clone()
    new_text = params.new_name.clone()

    edits: Map<Uri, Vec<TextEdit>> = {}

    // 1. Rename the heading itself
    edits[path_to_uri(path)].push(TextEdit {
        range:    heading.text_range,
        new_text: new_text.clone(),
    })

    // 2. Rewrite every [[note#OldText]] anchor link
    for each note in index.all_notes():
        for each link in note.wiki_links:
            if link.anchor (case-insensitive) == old_text AND
               index.resolve(&link.stem) == Found(path):
                edits[path_to_uri(note.path)].push(TextEdit {
                    range:    link.anchor_range,
                    new_text: new_text.clone(),
                })

    return Some(WorkspaceEdit { changes: Some(edits), ..Default::default() })
```

Anchor matching is case-insensitive (consistent with Go to Definition and
diagnostics). The `new_text` for the heading edit does not include `##` — it
replaces only `heading.text_range`.

**Returning `None`** when the cursor is not on a heading (no position match)
tells the editor there is nothing to rename here. Editors typically show a
"nothing to rename" message.

Advertise in `ServerCapabilities`:

```rust
rename_provider: Some(OneOf::Right(RenameOptions {
    prepare_provider: Some(true),
    work_done_progress_options: WorkDoneProgressOptions::default(),
})),
```

---

## Startup sequence (unchanged)

No changes to the startup sequence. Headings are parsed during the initial crawl
and on each `index(note)` call — both already happen.

---

## Testing

### Unit tests

**Parser (`src/parser/tests.rs`):**

| Test                                     | What it verifies                                                      |
| ---------------------------------------- | --------------------------------------------------------------------- |
| `heading_single`                         | `## My Heading` → one `Heading { text: "My Heading", level: 2 }`      |
| `heading_multiple_levels`                | Mixed ATX headings → correct order, levels, text                      |
| `heading_in_code_block_ignored`          | Headings inside fenced code blocks not extracted                      |
| `wiki_link_anchor_captured`              | `[[note#Section]]` → `anchor: Some("Section")`, `stem: "note"`        |
| `wiki_link_no_anchor`                    | `[[note]]` → `anchor: None`                                           |
| `wiki_link_alias_and_anchor`             | `[[note#Section\|alias]]` → `anchor: Some("Section")`, `stem: "note"` |
| `wiki_link_empty_anchor_treated_as_none` | `[[note#]]` → `anchor: None`                                          |

**Handlers (`src/handlers.rs` inline tests):**

| Test                                     | What it verifies                                                         |
| ---------------------------------------- | ------------------------------------------------------------------------ |
| `definition_anchor_navigates_to_heading` | `[[b#Section]]`, b.md has `## Section` → Location points to heading line |
| `definition_anchor_not_found_falls_back` | `[[b#Missing]]` → Location points to file top (no error)                 |
| `definition_no_anchor_unchanged`         | `[[b]]` → Location is file top (existing behaviour preserved)            |
| `anchor_diagnostic_missing`              | `[[b#Missing]]`, b.md has no "Missing" heading → Warning diagnostic      |
| `anchor_diagnostic_present`              | `[[b#Exists]]`, b.md has `## Exists` → no anchor diagnostic              |
| `anchor_diagnostic_case_insensitive`     | `[[b#my section]]` matches `## My Section` → no diagnostic               |
| `document_symbols_returns_headings`      | Note with 3 headings → 3 `DocumentSymbol`s with correct text and level   |
| `document_symbols_empty`                 | Note with no headings → empty vec                                        |
| `workspace_symbols_filtered`             | Query "sec" matches headings containing "sec" (case-insensitive)         |
| `workspace_symbols_empty_query`          | Empty query → all headings across all notes                              |

### Integration tests

**`tests/definition.rs` (extend existing):**

| Test                     | What it verifies                                                             |
| ------------------------ | ---------------------------------------------------------------------------- |
| `definition_with_anchor` | Full round-trip: `[[b#My Section]]`, b.md has the heading → correct Location |

**`tests/diagnostics.rs` (extend existing):**

| Test                         | What it verifies                                      |
| ---------------------------- | ----------------------------------------------------- |
| `broken_anchor_diagnostic`   | `[[note#Nonexistent]]` → Warning with correct message |
| `valid_anchor_no_diagnostic` | `[[note#Real Heading]]` → no anchor diagnostic        |

**`tests/symbols.rs` (new file):**

| Test                           | What it verifies                                    |
| ------------------------------ | --------------------------------------------------- |
| `document_symbols_round_trip`  | Request for a file → correct headings returned      |
| `workspace_symbols_round_trip` | Query returns matching headings from multiple files |

**Handlers (`src/handlers.rs` inline tests — rename):**

| Test                                    | What it verifies                                                                    |
| --------------------------------------- | ----------------------------------------------------------------------------------- |
| `rename_heading_updates_heading_text`   | Cursor on heading → WorkspaceEdit rewrites `text_range` in heading file             |
| `rename_heading_updates_anchor_links`   | Two files with `[[note#OldText]]` → both `anchor_range` edits included              |
| `rename_heading_case_insensitive_match` | `[[note#old text]]` matches `## Old Text` → edit included                           |
| `rename_heading_no_match_returns_none`  | Cursor not on a heading → `None`                                                    |
| `prepare_rename_returns_text_range`     | Cursor on heading → `RangeWithPlaceholder { range: text_range, placeholder: text }` |

**`tests/rename.rs` (extend existing):**

| Test                        | What it verifies                                                               |
| --------------------------- | ------------------------------------------------------------------------------ |
| `heading_rename_round_trip` | Full round-trip: rename heading, verify WorkspaceEdit rewrites heading + links |
