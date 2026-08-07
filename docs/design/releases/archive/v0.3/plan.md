# v0.3 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                                   | Status | Notes                     |
| -------------------------------------- | ------ | ------------------------- |
| 1 — Document Symbols                   | Done   |                           |
| 2 — Workspace Symbols                  | Done   |                           |
| 3 — Prepare Rename                     | Done   |                           |
| 4 — Heading Rename (+ GFM slug update) | Done   | slug applied throughout   |
| 5 — Anchor Completion                  | Done   |                           |
| 6 — Integration tests                  | Done   |                           |

## Approach

All steps follow TDD:

1. Write all unit tests for the deliverable first — stub the function signature
   if needed to compile
2. Run `cargo test` and confirm the new tests **fail** before writing any
   implementation
3. Implement until all tests pass, then run `cargo clippy -- -D warnings`

Step 5 must follow the same cycle: write the integration tests, confirm they
fail, then make them pass.

---

## Step 1 — Document Symbols

Implement `textDocument/documentSymbol`, delivering US-11. This is the simplest
of the three new handlers — it reads the headings already stored in the note
and maps them to LSP symbols with no index mutations or cross-file lookups.

**Deliverables:**

- Add `handle_document_symbols(params: DocumentSymbolParams, index: &NoteIndex) -> Option<DocumentSymbolResponse>`
  to `src/handlers.rs`:
  - Resolve path from `params.text_document.uri`; return `None` if not a `file://` URI
  - Look up `index.get_note(&path)`; return `None` if absent
  - Map each `Heading` in document order to `SymbolInformation`:
    - `name`: `heading.text`
    - `kind`: `SymbolKind::STRING`
    - `location`: `Location { uri: path_to_uri(&path), range: heading.range }`
    - `container_name`, `tags`, `deprecated`: `None`
  - Return `Some(DocumentSymbolResponse::Flat(symbols))`
- Add `document_symbol_provider: Some(OneOf::Left(true))` to `ServerCapabilities`
  in `src/server/mod.rs`
- Add `"textDocument/documentSymbol"` arm to `dispatch_request`; deserialize
  `DocumentSymbolParams`, call `handle_document_symbols`, send the result

**Unit tests:**

| Test                                         | What it verifies                                           |
| -------------------------------------------- | ---------------------------------------------------------- |
| `document_symbols_returns_all_headings`      | H1, H2, H3 note → three symbols in document order         |
| `document_symbols_note_absent_returns_none`  | URI not in index → `None`                                  |
| `document_symbols_no_headings_returns_empty` | Note with no headings → `Some(Flat([]))`                   |
| `document_symbols_kind_is_string`            | Every symbol has `kind == SymbolKind::STRING`              |
| `document_symbols_range_matches_heading`     | Symbol `location.range` matches the full heading line range |

> **Manual checkpoint:** Open a Markdown file with several headings of mixed
> levels in an editor with an Outline panel (VS Code, Zed). Trigger Document
> Symbols (`Cmd+Shift+O` / `Ctrl+Shift+O`). Confirm the panel lists every
> heading. Click one and confirm the editor jumps to that line.

---

## Step 2 — Workspace Symbols

Implement `workspace/symbol`, delivering US-12. Extends the same heading data to
a cross-file search: all headings across all indexed notes are searchable by the
query string.

**Deliverables:**

- Add `handle_workspace_symbols(params: WorkspaceSymbolParams, index: &NoteIndex) -> Vec<SymbolInformation>`
  to `src/handlers.rs`:
  - Lower-case `params.query`
  - For each `note` in `index.all_notes()`, for each `heading` in `note.headings`:
    - Include if `query.is_empty()` or `heading.text.to_lowercase().contains(&query)`
  - Map each match to `SymbolInformation`:
    - `name`: `heading.text`
    - `kind`: `SymbolKind::STRING`
    - `location`: `Location { uri: path_to_uri(&note.path), range: heading.range }`
    - `container_name`: `Some(note.path.file_name().to_string_lossy().into_owned())`
    - `tags`, `deprecated`: `None`
  - Return the collected list (no guaranteed order; editors re-rank by relevance)
- Add `workspace_symbol_provider: Some(OneOf::Left(true))` to `ServerCapabilities`
- Add `"workspace/symbol"` arm to `dispatch_request`; deserialize
  `WorkspaceSymbolParams`, call `handle_workspace_symbols`, send the result

**Unit tests:**

| Test                                        | What it verifies                                                  |
| ------------------------------------------- | ----------------------------------------------------------------- |
| `workspace_symbols_empty_query_returns_all` | Empty query → every heading from every indexed note               |
| `workspace_symbols_query_filters`           | Query `"intro"` → only headings whose text contains `"intro"` (case-insensitive) |
| `workspace_symbols_no_match_returns_empty`  | Query with no matching heading → empty vec                        |
| `workspace_symbols_container_is_filename`   | `container_name` equals the note's filename, not the full path    |
| `workspace_symbols_multiple_notes`          | Headings from two different notes both appear in results          |

> **Manual checkpoint:** Open an editor against a vault with several notes, each
> having multiple headings. Trigger the workspace symbol search (`Cmd+T` in Zed,
> `Ctrl+T` in VS Code). Type part of a heading name from a note you are not
> currently editing. Confirm the result appears, the container shows the correct
> filename, and navigating to it opens the right file at the right line.

---

## Step 3 — Prepare Rename

Implement `textDocument/prepareRename`, the first half of US-28. When the cursor
is on a heading line, this tells the editor which text range will be replaced and
pre-fills the rename input with the current heading text. When the cursor is
elsewhere, it vetoes the rename before the editor asks for the new name.

This step also introduces the `rename_provider` capability advertisement (with
`prepare_provider: true`) and stubs `handle_rename` to return `None` so the
server compiles and responds gracefully to `textDocument/rename` calls before
Step 4 fills in the real logic.

**Deliverables:**

- Add `handle_prepare_rename(params: TextDocumentPositionParams, index: &NoteIndex) -> Option<PrepareRenameResponse>`
  to `src/handlers.rs`:
  - Resolve path and cursor `pos` from params
  - Look up `note` via `index.get_note(&path)`; return `None` if absent
  - Find a heading where `heading.range.start.line == pos.line`; return `None`
    if none found
  - Return `Some(PrepareRenameResponse::RangeWithPlaceholder { range: heading.text_range, placeholder: heading.text.clone() })`
- Add stub `handle_rename(params: RenameParams, index: &NoteIndex) -> Option<WorkspaceEdit>`
  that returns `None` — will be replaced in Step 4
- Add `rename_provider: Some(OneOf::Right(RenameOptions { prepare_provider: Some(true), work_done_progress_options: Default::default() }))`
  to `ServerCapabilities`
- Add `"textDocument/prepareRename"` and `"textDocument/rename"` arms to
  `dispatch_request`

**Unit tests:**

| Test                                     | What it verifies                                                                    |
| ---------------------------------------- | ----------------------------------------------------------------------------------- |
| `prepare_rename_on_heading_returns_range`| Cursor on `## My Heading` → `RangeWithPlaceholder { range: text_range, placeholder: "My Heading" }` |
| `prepare_rename_off_heading_returns_none`| Cursor on a prose line → `None`                                                     |
| `prepare_rename_note_absent_returns_none`| URI not in index → `None`                                                           |
| `prepare_rename_range_is_text_not_markers`| Returned range covers the heading text, not the `## ` markers                      |

> **Manual checkpoint:** Open a note in an editor. Place the cursor on a heading
> line and trigger Rename Symbol (`F2` in VS Code / Zed). Confirm the rename
> input appears with the heading text pre-filled and the selection covers just
> the text, not the `#` markers. Place the cursor on a prose paragraph and
> trigger Rename Symbol — confirm the editor shows an error or the dialog does
> not appear.

---

## Step 4 — Heading Rename (with GFM slug anchors)

Replace the stub `handle_rename` with the real implementation, completing US-28.
Also updates the anchor-matching logic in `compute_diagnostics` and
`handle_definition` to use GFM slugification (enabling multi-word headings to
work correctly throughout the server).

For the heading under the cursor, `handle_rename` builds a `WorkspaceEdit` that
rewrites (a) the heading text in the source file, (b) anchor-only self-links
within the same file, and (c) all incoming `[text](note.md#old-slug)` links
across the workspace.

**Deliverables:**

- Add `fn slug(text: &str) -> String` helper to `src/handlers.rs`:
  - Keep alphanumeric, space, and hyphen characters; strip everything else
  - Lowercase the result
  - Replace spaces with hyphens
  - Example: `slug("My Section") == "my-section"`

- Update anchor matching in `compute_diagnostics`: change
  `h.text.to_lowercase() == anchor.to_lowercase()`
  → `slug(&h.text) == slug(anchor)`

- Update anchor matching in `handle_definition`: same change

- Replace the stub `handle_rename` in `src/handlers.rs` with the full implementation:
  - Resolve path, cursor `pos`, and `new_name` from `params`
  - Look up `note`; find heading where `heading.range.start.line == pos.line`;
    return `None` if absent
  - Let `old_slug = slug(&heading.text)`
  - **Heading text:** push `TextEdit { range: heading.text_range, new_text: new_name.clone() }`
    under the source file's URI — heading displays the human-readable name
  - **Self-links:** for each `link` in `note.md_links` where `link.target.is_empty()`:
    if `link.anchor.as_deref().map(slug) == Some(old_slug.clone())`
    and `link.anchor_range` is `Some(anchor_range)`, push
    `TextEdit { range: anchor_range, new_text: slug(new_name) }` under the
    source file's URI
  - **Incoming links:** for each `located` in `index.links_to(&path)`:
    if `slug(anchor) == old_slug` and `anchor_range` is `Some`, push
    `TextEdit { range: anchor_range, new_text: slug(new_name) }` under
    `located.source_path`'s URI
  - Return `Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })`

**Unit tests** (use multi-word headings and slug anchors):

| Test                                         | What it verifies                                                                    |
| -------------------------------------------- | ----------------------------------------------------------------------------------- |
| `rename_heading_edits_text`                  | `text_range` rewritten to `new_name`; anchor-bearing links updated to `slug(new_name)` |
| `rename_heading_updates_incoming_anchor`     | `[text](note.md#old-heading)` anchor range rewritten to `slug(new_name)`           |
| `rename_heading_updates_self_anchor`         | Same-file `[text](#old-heading)` anchor range rewritten to `slug(new_name)`        |
| `rename_heading_case_insensitive_match`      | Link anchor `OLD-HEADING` matches heading `Old Heading` — anchor updated           |
| `rename_heading_non_matching_anchor_skipped` | Link with a different anchor is absent from the edit                               |
| `rename_heading_no_heading_at_cursor_none`   | Cursor on a prose line → `None`                                                    |

> **Manual checkpoint:** Open two notes in an editor. In note B, add two
> headings: `## Old Heading` and `## Details`. In note A, add
> `[see it](b.md#old-heading)`. In note B also add `[jump](#old-heading)`
> (a self-link). Rename the `Old Heading` heading via `F2` to `New Heading`.
> Confirm: the heading text in B changes to `New Heading`, the anchor in A
> updates to `#new-heading`, and the self-link in B updates to `#new-heading`
> — all in one atomic operation with no broken-link diagnostics afterwards.
> Verify that `## Details` links are not touched.

---

## Step 5 — Anchor Completion

Extend `handle_completion` to serve heading suggestions when the cursor is inside
a link destination after `#`, delivering US-45. The `#` is added as a second
completion trigger character alongside `(`.

**Deliverables:**

- Add `fn check_anchor_trigger(content: &str, pos: Position) -> Option<String>`
  to `src/handlers.rs`:
  - Scan the current line up to cursor, find the last `](`
  - In the substring after `](`, find the last `#`
  - Return `Some(path)` where `path` is the text between `](` and `#`; `None`
    if neither `](` nor `#` is found in that order

- Update `handle_completion` in `src/handlers.rs`:
  - Call `check_anchor_trigger` before `check_link_trigger`
  - If anchor context: resolve `typed_path` via `index.resolve(&current_path, &typed_path)`;
    return `vec![]` if unresolved; otherwise look up the target note and map its
    headings to `CompletionItem`:
    - `label`: `heading.text` (e.g. `"My Section"`)
    - `filter_text`: `Some(heading.text.clone())`
    - `insert_text`: `Some(slug(&heading.text))` (e.g. `"my-section"`, no leading `#`)
    - `kind`: `Some(CompletionItemKind::REFERENCE)`
    - `detail`: `Some(format!("#{}", slug(&heading.text)))`
  - If file context (existing path): existing file-completion logic unchanged

- Update `completion_provider` in `src/server/mod.rs`:
  ```rust
  CompletionOptions {
      trigger_characters: Some(vec!["(".to_string(), "#".to_string()]),
      ..Default::default()
  }
  ```

**Unit tests:**

| Test                                          | What it verifies                                                             |
| --------------------------------------------- | ---------------------------------------------------------------------------- |
| `anchor_completion_returns_headings`          | `](a.md#` context → one item per heading in a.md                            |
| `anchor_completion_label_is_heading_text`     | Item label equals the heading text as written ("My Section")                 |
| `anchor_completion_insert_is_slug`            | `insert_text` = GFM slug with no leading `#` ("my-section")                 |
| `anchor_completion_unknown_file_empty`        | Path before `#` does not resolve → empty list                                |
| `anchor_completion_no_headings_empty`         | Target file has no headings → empty list                                     |
| `anchor_completion_does_not_fire_on_plain_hash` | `#` not preceded by `](path` → no anchor items (falls through to file completion check) |

> **Manual checkpoint:** In a note with `[link](b.md`, position cursor after
> `(` and type `b.md#`. Heading suggestions from `b.md` should appear
> immediately. Confirm the label shows the heading text (e.g. "My Section") and
> selecting it inserts `my-section` (slug, no `#`). Confirm that typing `#` at
> the start of a line (to write a new heading) does not trigger the picker.

---

## Step 6 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/lsp.rs` additions — all integration tests listed below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                          | What it verifies                                                                      |
| --------------------------------------------- | ------------------------------------------------------------------------------------- |
| `test_document_symbols_lists_headings`        | `textDocument/documentSymbol` returns one symbol per heading in the file              |
| `test_document_symbols_empty_for_no_headings` | File with no headings returns an empty flat list, not null                            |
| `test_workspace_symbols_query`                | `workspace/symbol` with a query string returns only matching headings                 |
| `test_workspace_symbols_empty_query`          | Empty query returns headings from all indexed notes                                   |
| `test_prepare_rename_on_heading`              | `textDocument/prepareRename` on a heading returns a non-null range response           |
| `test_prepare_rename_off_heading`             | `textDocument/prepareRename` on prose returns null                                    |
| `test_rename_heading_updates_anchor_links`    | `textDocument/rename` rewrites slug anchors in other files to `slug(new_name)`        |
| `test_rename_heading_updates_self_links`      | `textDocument/rename` rewrites anchor-only self-links to `slug(new_name)`             |
| `test_anchor_completion`                      | `](file.md#` triggers completion; items carry heading-text labels and slug inserts    |

> **Manual checkpoint (full session):** Open a vault in an editor. (1) Navigate
> to a note with headings and open the Outline panel — confirm all headings
> appear. (2) Use workspace symbol search and type part of a heading from a
> different note — confirm it appears and navigating to it lands on the correct
> line. (3) Click a `[text](note.md#heading)` link and trigger Go to Definition
> — confirm it jumps to the heading line. (4) Rename a heading that is
> referenced by links in two other files and by a self-link — confirm all three
> are updated atomically with no broken-link diagnostics. (5) In a note, type
> `[link](b.md#` and confirm heading completions appear; select one and confirm
> the slug is inserted. Confirm all v0.1 and v0.2 capabilities are unaffected.

---

## Done — v0.3 complete

| Story | Feature                                                  | Delivered in step |
| ----- | -------------------------------------------------------- | ----------------- |
| US-06 | Go to Definition navigates to heading anchor             | Already done      |
| US-08 | Diagnostic for missing heading anchor                    | Already done      |
| US-11 | Document Symbols — headings in current file              | Step 1            |
| US-12 | Workspace Symbols — search headings across files         | Step 2            |
| US-28 | Rename heading → all anchor links updated                | Steps 3 + 4       |
| US-45 | Anchor completions — heading picker inserts GFM slug     | Step 5            |
