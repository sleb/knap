# v0.3 Design — Heading Navigation & Anchors

Covers the stories in the v0.3 release:

| Story | Feature                                                                    |
| ----- | -------------------------------------------------------------------------- |
| US-06 | `[text](note.md#heading)` — Go to Definition navigates to the heading line |
| US-08 | Diagnostic when a heading anchor no longer exists in the target file       |
| US-11 | Document Symbols — jump to any heading within the current file             |
| US-12 | Workspace Symbols — search headings across all files                       |
| US-28 | Rename a heading → all `[text](note.md#old-heading)` links updated         |

---

## Goal

Navigate within notes, not just between them. A writer can jump to any heading
inside a file via Document Symbols, search headings across the whole workspace
via Workspace Symbols, and rename a heading confident that every
`[text](note.md#old-heading)` anchor in the vault is updated atomically.

---

## What Is Already Implemented

US-06 and US-08 were implemented as part of the heading infrastructure landed
in v0.1 and are formally claimed here:

- **US-06 (Go to Definition with anchor):** `handle_definition` already checks
  `link.anchor`, looks up the target note's headings, and returns
  `Location { uri, range: heading.range }`. No changes needed.

- **US-08 (anchor diagnostic):** `compute_diagnostics` already emits a `WARNING`
  at `link.anchor_range` when a `Found` link has an anchor that doesn't match
  any heading in the target file. No changes needed.

The v0.3 work is exclusively the three new handlers — Document Symbols,
Workspace Symbols, and heading Rename — plus the protocol-handler changes to
advertise them.

---

## Parser Changes

None. The `Heading` type already carries all fields needed:

```rust
pub struct Heading {
    pub text: String,         // raw heading text, e.g. "My Section"
    pub level: u8,            // ATX heading level 1–6
    pub range: LspRange,      // full heading line (for navigation, DocumentSymbol range)
    pub text_range: LspRange, // text only, excluding "## " prefix (for rename selection)
}
```

`Note.headings` is populated in document order by `extract_body_elements`.

---

## Index Changes

None. The existing read methods are sufficient:

- `get_note(path) → Option<&Note>` — look up a note's headings for document
  symbols and to identify the heading under the cursor.
- `all_notes() → impl Iterator<Item = &Note>` — enumerate all headings for
  workspace symbols.
- `links_to(path) → &[LocatedLink]` — find all incoming links for heading rename;
  `LocatedLink.md_link.anchor` and `.anchor_range` carry what we need.

---

## Handler Changes

### `handle_document_symbols` (new — US-11)

Returns a flat list of `SymbolInformation` — one entry per heading in the
requested file, in document order.

```rust
pub fn handle_document_symbols(
    params: DocumentSymbolParams,
    index: &NoteIndex,
) -> Option<DocumentSymbolResponse>
```

Steps:

1. Extract path from `params.text_document.uri`. Return `None` if not a
   `file://` URI.
2. Look up the note with `index.get_note(&path)`. Return `None` if absent.
3. Map `note.headings` to `SymbolInformation`:
   - `name`: `heading.text`
   - `kind`: `SymbolKind::STRING`
   - `location`: `Location { uri: path_to_uri(&path), range: heading.range }`
   - `container_name`: `None` (flat list; nesting deferred)
   - `tags`, `deprecated`: `None`
4. Return `Some(DocumentSymbolResponse::Flat(symbols))`.

Empty headings list → `Some(Flat([]))` (not `None`; the file exists, it just
has no headings).

---

### `handle_workspace_symbols` (new — US-12)

Returns all headings across all indexed notes that match the query string.

```rust
pub fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    index: &NoteIndex,
) -> Vec<SymbolInformation>
```

Steps:

1. Lower-case `params.query` (empty string matches everything).
2. For each `note` in `index.all_notes()`, for each `heading` in
   `note.headings`:
   - Include if `query.is_empty() || heading.text.to_lowercase().contains(&query)`.
3. Map matching headings to `SymbolInformation`:
   - `name`: `heading.text`
   - `kind`: `SymbolKind::STRING`
   - `location`: `Location { uri: path_to_uri(&note.path), range: heading.range }`
   - `container_name`: `Some(note.path.file_name().to_string_lossy().into_owned())`
   - `tags`, `deprecated`: `None`
4. Return the collected list (order is unspecified; editors re-sort by relevance).

---

### `handle_prepare_rename` (new — US-28)

Tells the editor which range will be replaced and pre-fills the rename input.
Called before `handle_rename`; returning `None` vetoes the rename.

```rust
pub fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse>
```

Steps:

1. Extract path and cursor `pos` from params.
2. Look up `note` with `index.get_note(&path)`. Return `None` if absent.
3. Find the first heading where `heading.range.start.line == pos.line`. Return
   `None` if no heading is on the cursor line.
4. Return `Some(PrepareRenameResponse::RangeWithPlaceholder {
       range: heading.text_range,
       placeholder: heading.text.clone(),
   })`.

The `RangeWithPlaceholder` form pre-fills the editor's rename input with the
current heading text, so the writer edits it directly rather than typing from
scratch.

---

### `handle_rename` (new — US-28)

Returns a `WorkspaceEdit` that renames the heading under the cursor and updates
every anchor link that references it.

```rust
pub fn handle_rename(
    params: RenameParams,
    index: &NoteIndex,
) -> Option<WorkspaceEdit>
```

Steps:

1. Extract path, cursor `pos`, and `new_name` from params.
2. Look up `note` with `index.get_note(&path)`. Return `None` if absent.
3. Find the heading where `heading.range.start.line == pos.line`. Return `None`
   if the cursor is not on a heading line.
4. Let `old_text = heading.text.to_lowercase()`. Build
   `HashMap<Uri, Vec<TextEdit>>`:

   **a. Heading text in the source file:**
   ```
   TextEdit { range: heading.text_range, new_text: new_name.clone() }
   ```
   keyed under `path_to_uri(&path)`.

   **b. Anchor-only self-links inside the same file:**
   For each `link` in `note.md_links` where `link.target.is_empty()`:
   - If `link.anchor.as_deref().map(|a| a.to_lowercase()) == Some(old_text.as_str())`
   - And `link.anchor_range` is `Some(anchor_range)`:
     ```
     TextEdit { range: anchor_range, new_text: new_name.clone() }
     ```
   keyed under `path_to_uri(&path)` (same file, different ranges from step a).

   **c. Incoming links from other files:**
   For each `located` in `index.links_to(&path)`:
   - If `located.md_link.anchor.as_deref().map(|a| a.to_lowercase()) == Some(old_text.as_str())`
   - And `located.md_link.anchor_range` is `Some(anchor_range)`:
     ```
     TextEdit { range: anchor_range, new_text: new_name.clone() }
     ```
   keyed under `path_to_uri(&located.source_path)`.

5. Return `Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })`.

**Anchor matching is case-insensitive**, consistent with how `compute_diagnostics`
and `handle_definition` match anchors to headings. A link `[text](note.md#My Section)`
and a heading `## my section` are considered the same anchor.

**Only links with a recorded `anchor_range` are updated.** In practice every
link parsed with a `#` has an `anchor_range`, so this guard is just defensive.

---

## Protocol Handler Changes

### Capabilities

Add three entries to `ServerCapabilities` in `initialize`:

```rust
document_symbol_provider: Some(OneOf::Left(true)),
workspace_symbol_provider: Some(OneOf::Left(true)),
rename_provider: Some(OneOf::Right(RenameOptions {
    prepare_provider: Some(true),
    work_done_progress_options: Default::default(),
})),
```

`prepare_provider: Some(true)` tells editors to call `textDocument/prepareRename`
before `textDocument/rename`, which lets the server veto renames attempted on
non-heading positions and pre-fill the editor's rename input.

### Dispatch

Add four cases to `dispatch_request`:

```rust
"textDocument/documentSymbol" => {
    let result = serde_json::from_value::<DocumentSymbolParams>(req.params)
        .ok()
        .and_then(|params| handlers::handle_document_symbols(params, index));
    connection.sender.send(Message::Response(Response::new_ok(req.id, result)))?;
}
"workspace/symbol" => {
    let result = serde_json::from_value::<WorkspaceSymbolParams>(req.params)
        .ok()
        .map(|params| handlers::handle_workspace_symbols(params, index))
        .unwrap_or_default();
    connection.sender.send(Message::Response(Response::new_ok(req.id, result)))?;
}
"textDocument/prepareRename" => {
    let result = serde_json::from_value::<TextDocumentPositionParams>(req.params)
        .ok()
        .and_then(|params| handlers::handle_prepare_rename(params, index));
    connection.sender.send(Message::Response(Response::new_ok(req.id, result)))?;
}
"textDocument/rename" => {
    let result = serde_json::from_value::<RenameParams>(req.params)
        .ok()
        .and_then(|params| handlers::handle_rename(params, index));
    connection.sender.send(Message::Response(Response::new_ok(req.id, result)))?;
}
```

---

## Testing

### Unit tests (`src/handlers.rs`)

| Test                                          | What it verifies                                                                  |
| --------------------------------------------- | --------------------------------------------------------------------------------- |
| `document_symbols_returns_all_headings`       | Note with H1, H2, H3 → three symbols in document order                           |
| `document_symbols_note_absent_returns_none`   | URI not in index → `None`                                                         |
| `document_symbols_no_headings_returns_empty`  | Note with no headings → `Some(Flat([]))`                                          |
| `document_symbols_kind_is_string`             | Each symbol has `kind == SymbolKind::STRING`                                      |
| `workspace_symbols_empty_query_returns_all`   | Empty query → every heading from every indexed note                               |
| `workspace_symbols_query_filters`             | Query "intro" → only headings containing "intro" (case-insensitive)               |
| `workspace_symbols_no_match_returns_empty`    | Query with no match → empty vec                                                   |
| `workspace_symbols_container_is_filename`     | `container_name` equals the note's filename                                       |
| `prepare_rename_on_heading_returns_range`     | Cursor on `## My Heading` → `RangeWithPlaceholder { range: text_range, placeholder: "My Heading" }` |
| `prepare_rename_off_heading_returns_none`     | Cursor on a prose line → `None`                                                   |
| `rename_heading_edits_text`                   | Heading text range rewritten to `new_name`                                        |
| `rename_heading_updates_incoming_anchor`      | Incoming `[text](note.md#Old Heading)` → anchor rewritten to new name             |
| `rename_heading_updates_self_anchor`          | Same-file `[text](#Old Heading)` → anchor rewritten to new name                   |
| `rename_heading_case_insensitive_match`       | Link anchor `old heading` matches heading `Old Heading` → both updated            |
| `rename_heading_non_matching_anchor_skipped`  | Link with a different anchor → not included in the edit                           |
| `rename_heading_no_heading_at_cursor_none`    | Cursor not on a heading line → `None`                                             |

### Integration tests (`tests/lsp.rs`)

| Test                                          | What it verifies                                                                   |
| --------------------------------------------- | ---------------------------------------------------------------------------------- |
| `test_document_symbols_lists_headings`        | `textDocument/documentSymbol` returns one entry per heading in the file            |
| `test_workspace_symbols_query`                | `workspace/symbol` with a query string returns only matching heading names         |
| `test_prepare_rename_on_heading`              | `textDocument/prepareRename` on a heading returns a non-null range response        |
| `test_prepare_rename_off_heading`             | `textDocument/prepareRename` on prose returns null                                 |
| `test_rename_heading_updates_anchor_links`    | `textDocument/rename` returns edits that rewrite anchors in other files            |
| `test_rename_heading_updates_self_links`      | `textDocument/rename` returns edits for anchor-only self-links in the same file    |
