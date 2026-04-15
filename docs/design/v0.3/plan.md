# v0.3 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                          | Status      | Notes |
| ----------------------------- | ----------- | ----- |
| 1 — Parser: headings + anchor | Not started |       |
| 2 — Go to Definition (US-06)  | Not started |       |
| 3 — Broken anchor diagnostics | Not started |       |
| 4 — Document Symbols (US-11)  | Not started |       |
| 5 — Workspace Symbols (US-12) | Not started |       |
| 6 — Heading rename (US-28)    | Not started |       |

---

## Step 1 — Parser: headings + anchor capture

Add `Heading` and extract headings from each note. Capture the anchor portion
of `[[note#Section]]` links instead of silently discarding it.

**Deliverables:**

- `Heading { text, level, range, text_range }` added to `src/parser/mod.rs`;
  `text_range` covers only the heading text (excludes `## ` prefix), used by
  the rename handler
- `Note` gains `headings: Vec<Heading>`
- `WikiLink` gains `anchor: Option<String>` (text after `#`, before `|`, trimmed;
  empty string normalised to `None`) and `anchor_range: Option<LspRange>` (byte
  range of just the anchor text, used by the rename handler and future code action)
- `parse()` calls a new `extract_headings()` function using pulldown-cmark's
  offset iterator
- `scan_wiki_links()` captures the `#anchor` portion and its byte range when
  splitting on `#`
- `knap parse <file>` CLI output includes headings

**Unit tests** (`src/parser/tests.rs`):

| Test                                     | What it verifies                                                      |
| ---------------------------------------- | --------------------------------------------------------------------- |
| `heading_single`                         | `## My Heading` → `Heading { text: "My Heading", level: 2 }`          |
| `heading_multiple_levels`                | Mixed ATX headings → correct order, levels, text                      |
| `heading_in_code_block_ignored`          | Headings inside fenced code blocks are not extracted                  |
| `heading_text_range`                     | `## My Heading` → `text_range` covers "My Heading", not "## "         |
| `wiki_link_anchor_captured`              | `[[note#Section]]` → `anchor: Some("Section")`, `stem: "note"`        |
| `wiki_link_anchor_range`                 | `[[note#Section]]` → `anchor_range` covers "Section" characters only  |
| `wiki_link_no_anchor`                    | `[[note]]` → `anchor: None`, `anchor_range: None`                     |
| `wiki_link_alias_and_anchor`             | `[[note#Section\|alias]]` → `anchor: Some("Section")`, `stem: "note"` |
| `wiki_link_empty_anchor_treated_as_none` | `[[note#]]` → `anchor: None`, `anchor_range: None`                    |

> **Manual checkpoint:** `cargo run -- parse <file>` on a file containing ATX
> headings and `[[note#anchor]]` links should print the headings and show the
> anchor field on the relevant wiki-links.

---

## Step 2 — Go to Definition with heading navigation (US-06)

Update `handle_definition` to navigate to the target heading line when a link
carries an anchor. No index changes needed — headings are already on `Note`.

**Deliverables:**

- `handle_definition` checks `link.anchor`; if `Some`, finds the first
  case-insensitive matching heading in the target note and returns its range
- If the anchor is present but no heading matches, falls back to the file top
  (same behaviour as a link without an anchor)
- `textDocument/definition` routing unchanged

**Unit tests** (inline in `src/handlers.rs`):

| Test                                     | What it verifies                                                         |
| ---------------------------------------- | ------------------------------------------------------------------------ |
| `definition_anchor_navigates_to_heading` | `[[b#Section]]`, b.md has `## Section` → Location points to heading line |
| `definition_anchor_not_found_falls_back` | `[[b#Missing]]` → Location points to file top (line 0)                   |
| `definition_no_anchor_unchanged`         | `[[b]]` → Location is file top (existing behaviour preserved)            |

**Integration test** (extend `tests/definition.rs`):

| Test                     | What it verifies                                                              |
| ------------------------ | ----------------------------------------------------------------------------- |
| `definition_with_anchor` | Full round-trip: `[[b#My Section]]` on a file that has the heading → Location |

> **Manual checkpoint:** in an editor, `[[Note#Heading]]` Go to Definition
> jumps to the heading line, not the file top. A link to a nonexistent heading
> still navigates to the file top with no error.

---

## Step 3 — Broken anchor diagnostics (US-08)

Extend `compute_diagnostics` to emit a warning when a link's anchor does not
match any heading in the target note.

**Deliverables:**

- `compute_diagnostics` adds a third branch inside the `Found` arm: if
  `link.anchor` is `Some(anchor)` and the target note has no heading that
  matches `anchor` (case-insensitive), emit a `Warning` at `link.inner_range`
- Diagnostic message: `Heading not found: '#Anchor' in '[[stem#Anchor]]'`

**Unit tests** (inline in `src/handlers.rs`):

| Test                                 | What it verifies                                                 |
| ------------------------------------ | ---------------------------------------------------------------- |
| `anchor_diagnostic_missing`          | `[[b#Missing]]`, b.md has no "Missing" heading → Warning emitted |
| `anchor_diagnostic_present`          | `[[b#Exists]]`, b.md has `## Exists` → no extra diagnostic       |
| `anchor_diagnostic_case_insensitive` | `[[b#my section]]` matches `## My Section` → no diagnostic       |

**Integration tests** (extend `tests/diagnostics.rs`):

| Test                         | What it verifies                                      |
| ---------------------------- | ----------------------------------------------------- |
| `broken_anchor_diagnostic`   | `[[note#Nonexistent]]` → Warning with correct message |
| `valid_anchor_no_diagnostic` | `[[note#Real Heading]]` → no anchor diagnostic        |

> **Manual checkpoint:** link to a heading that exists — no warning. Delete or
> rename the heading — the warning appears on the next file save. Rename the
> file the link points to — the anchor diagnostic is gone (broken-link
> diagnostic takes priority).

---

## Step 4 — Document Symbols (US-11)

Implement `textDocument/documentSymbol` so editors can show an outline of
headings in the current file.

**Deliverables:**

- `handle_document_symbols(params: DocumentSymbolParams, index: &NoteIndex) → DocumentSymbolResponse`
  returns `DocumentSymbolResponse::Nested(Vec<DocumentSymbol>)` — flat list,
  one entry per heading
- `ServerCapabilities.document_symbol_provider = Some(OneOf::Left(true))`
- `dispatch_request` routes `textDocument/documentSymbol`

**Unit tests** (inline in `src/handlers.rs`):

| Test                                | What it verifies                                               |
| ----------------------------------- | -------------------------------------------------------------- |
| `document_symbols_returns_headings` | Note with 3 headings → 3 `DocumentSymbol`s, correct text/level |
| `document_symbols_empty`            | Note with no headings → empty vec                              |

**Integration test** (new `tests/symbols.rs`):

| Test                          | What it verifies                                             |
| ----------------------------- | ------------------------------------------------------------ |
| `document_symbols_round_trip` | Request for a file → correct headings returned by the server |

> **Manual checkpoint:** open a Markdown file with several ATX headings. The
> editor's outline panel (VS Code: Outline view; Zed: symbol picker) lists all
> headings. Clicking one navigates to that heading line.

---

## Step 5 — Workspace Symbols (US-12)

Implement `workspace/symbol` so editors can search headings across all files.

**Deliverables:**

- `handle_workspace_symbols(params: WorkspaceSymbolParams, index: &NoteIndex) → Vec<SymbolInformation>`
  filters by case-insensitive substring match on `params.query`; empty query
  returns all headings
- `ServerCapabilities.workspace_symbol_provider = Some(OneOf::Left(true))`
- `dispatch_request` routes `workspace/symbol`

**Unit tests** (inline in `src/handlers.rs`):

| Test                            | What it verifies                                                 |
| ------------------------------- | ---------------------------------------------------------------- |
| `workspace_symbols_filtered`    | Query "sec" matches headings containing "sec" (case-insensitive) |
| `workspace_symbols_empty_query` | Empty query → all headings across all indexed notes              |

**Integration test** (extend `tests/symbols.rs`):

| Test                           | What it verifies                                    |
| ------------------------------ | --------------------------------------------------- |
| `workspace_symbols_round_trip` | Query returns matching headings from multiple files |

> **Manual checkpoint:** trigger workspace symbol search in the editor (VS Code:
> `Cmd+T`; Zed: symbol picker). Type part of a heading — matching headings from
> across the workspace appear with the source file shown. Selecting one navigates
> to that heading.

---

## Step 6 — Heading rename (US-28)

Implement `textDocument/prepareRename` and `textDocument/rename` for headings.
When the cursor is on a heading, rename rewrites the heading text and updates
every `[[note#OldText]]` anchor link across the workspace.

**Deliverables:**

- `handle_prepare_rename(params, index) → Option<PrepareRenameResponse>`:
  if cursor falls within a heading's `range`, return
  `RangeWithPlaceholder { range: heading.text_range, placeholder: heading.text }`
- `handle_rename(params, index) → Option<WorkspaceEdit>`:
  if cursor is on a heading, build a `WorkspaceEdit` with:
  1. `TextEdit` at `heading.text_range` in the heading's file
  2. `TextEdit` at each `link.anchor_range` where the anchor matches the old
     heading text (case-insensitive) and the link resolves to the heading's file
- Returns `None` when cursor is not on a heading (nothing to rename)
- `ServerCapabilities.rename_provider` advertised with `prepare_provider: true`
- `dispatch_request` routes `textDocument/prepareRename` and `textDocument/rename`

**Unit tests** (inline in `src/handlers.rs`):

| Test                                    | What it verifies                                                                    |
| --------------------------------------- | ----------------------------------------------------------------------------------- |
| `rename_heading_updates_heading_text`   | Cursor on heading → WorkspaceEdit contains `TextEdit` at `text_range`               |
| `rename_heading_updates_anchor_links`   | Two files with `[[note#OldText]]` → both `anchor_range` edits included              |
| `rename_heading_case_insensitive_match` | `[[note#old text]]` matches `## Old Text` → anchor edit included                    |
| `rename_heading_no_match_returns_none`  | Cursor not on any heading → `None`                                                  |
| `prepare_rename_on_heading`             | Cursor on heading → `RangeWithPlaceholder { range: text_range, placeholder: text }` |
| `prepare_rename_not_on_heading`         | Cursor elsewhere → `None`                                                           |

**Integration test** (extend `tests/rename.rs`):

| Test                        | What it verifies                                                               |
| --------------------------- | ------------------------------------------------------------------------------ |
| `heading_rename_round_trip` | Full round-trip: rename heading, verify WorkspaceEdit rewrites heading + links |

> **Manual checkpoint:** place cursor on a heading line. Invoke rename (F2 /
> `<leader>rn`). The input is pre-filled with the heading text. Type a new name
> — all `[[note#OldText]]` links in the workspace update instantly. Cursor
> elsewhere → rename is a no-op (editor shows "nothing to rename").

---

## Done — v0.3 complete

At this point all five v0.3 user stories are implemented and tested:

| Story | Feature                             | Delivered in step |
| ----- | ----------------------------------- | ----------------- |
| US-06 | `[[Note#Heading]]` Go to Definition | Step 2            |
| US-08 | Broken anchor diagnostics           | Step 3            |
| US-11 | Document Symbols                    | Step 4            |
| US-12 | Workspace Symbols                   | Step 5            |
| US-28 | Heading rename                      | Step 6            |

Final check before tagging: run `cargo test`, run
`cargo clippy -- -D warnings`, then do a manual end-to-end session — verify
completion, definition (plain and anchored), references, file rename, heading
rename, and diagnostics (broken link, ambiguous, and broken anchor) all behave
correctly.
