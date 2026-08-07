# v0.9 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Parallel execution model

Steps 2A–2D are independent and can be assigned to separate agents running
simultaneously in distinct git worktrees. Each parallel step touches only
`src/handlers.rs` (adding a new function or extending an existing one) and its
unit-test block — no shared state. Step 3 is the serial merge: it collects
all four handler functions, resolves import conflicts, and wires capabilities
and dispatch arms into `src/server/mod.rs`.

**Merge contract for parallel agents:** Each parallel step must:

1. Add its new `lsp_types` imports at the top of `src/handlers.rs` in the
   existing `use lsp_types::{...}` block.
2. Add its handler function(s) **at the end of** `src/handlers.rs`, after the
   `handle_code_lens` block and before the `uri_to_path` / `path_to_uri`
   utilities.
3. Add its unit tests inside the `#[cfg(test)] mod tests { }` block.
4. Not touch `src/server/mod.rs` — that file is reserved for Step 3.

Following this contract means Step 3's merge conflicts are confined to the
`use lsp_types::{...}` import list and the test-module `use` block — both
additive, no logic conflicts.

---

## Status

| Step                    | Status  | Notes |
| ----------------------- | ------- | ----- |
| 1 — Parser + docs       | ✅ Done |       |
| 2A — Folding ranges     | ✅ Done |       |
| 2B — Selection range    | ✅ Done |       |
| 2C — Inlay hints        | ✅ Done |       |
| 2D — Code lens headings | ✅ Done |       |
| 3 — Wire capabilities   | ✅ Done |       |
| 4 — Integration tests   | ✅ Done |       |

---

## Step 1 — Parser extension and user-story stubs

Add `CodeFence` extraction to the parser (required by Step 2B) and add the
three missing user stories to `docs/USER_STORIES.md` (US-52, US-53, US-54 are
in the roadmap but not yet in that file). This step is serial and must complete
before Step 2A begins; Steps 2B, 2C, 2D can start in parallel with or
immediately after this step since they need no parser changes.

**Deliverables:**

- Add `pub struct CodeFence { pub start_line: u32, pub end_line: u32 }` to
  `src/parser/mod.rs`
- Add `pub code_fences: Vec<CodeFence>` field to `pub struct Note` in
  `src/parser/mod.rs`
- Extend `extract_body_elements` in `src/parser/mod.rs` to collect fenced code
  blocks: watch for `Event::Start(PdTag::CodeBlock(CodeBlockKind::Fenced(_)))`
  and `Event::End(TagEnd::CodeBlock)`; skip `CodeBlockKind::Indented`
- Update `parse()` in `src/parser/mod.rs` to pass `code_fences` through the
  returned `Note`
- Add US-52, US-53, US-54 to `docs/USER_STORIES.md` under the "Editor
  Experience" section

**Unit tests** (in `src/parser/mod.rs` tests):

| Test                                | What it verifies                                                  |
| ----------------------------------- | ----------------------------------------------------------------- |
| `code_fence_start_end_lines`        | fenced block captures correct `start_line` and `end_line`         |
| `code_fence_indented_block_skipped` | indented code block produces no `CodeFence`                       |
| `code_fence_empty_body_skipped`     | back-to-back fences with no content between them produce no entry |

> **Manual checkpoint:** No editor action yet — run `cargo test` and confirm
> all 3 new tests pass.

---

## Step 2A — Folding ranges (parallel, after Step 1)

Implement `handle_folding_ranges` using `note.headings` (existing) and
`note.code_fences` (added in Step 1). Start this step after Step 1 is merged
to main; it can otherwise run in parallel with Steps 2B, 2C, 2D.

**TDD cycle:**

1. Write all unit tests below first; stub `handle_folding_ranges`.
2. Run `cargo test` — new tests fail.
3. Implement; then `cargo clippy -- -D warnings`.

**Deliverables:**

- Add to `use lsp_types::{...}`:
  `FoldingRange, FoldingRangeKind, FoldingRangeParams`
- Add `pub(crate) fn handle_folding_ranges(params: FoldingRangeParams, index: &NoteIndex) -> Vec<FoldingRange>` to `src/handlers.rs`
- Helper (private): `fn last_content_line(content: &str) -> u32` — returns the
  zero-based line number of the last non-empty line in the document

**Unit tests:**

| Test                                      | What it verifies                                                 |
| ----------------------------------------- | ---------------------------------------------------------------- |
| `folding_h2_section_spans_to_next_h2`     | H2 at line 0 ends on the line before the next H2                 |
| `folding_nested_h3_ends_before_parent`    | H3 at line 2 ends before the parent H2 section ends              |
| `folding_last_heading_spans_to_doc_end`   | last heading section extends to the document's last content line |
| `folding_single_line_section_omitted`     | heading whose end equals its start line produces no range        |
| `folding_code_fence_emitted`              | fenced code block produces one `FoldingRangeKind::Region` range  |
| `folding_no_headings_returns_fences_only` | document with no headings returns only code-fence ranges         |

> **Manual checkpoint:** No editor action yet — run `cargo test` and confirm all
> 6 new tests pass.

---

## Step 2B — Selection range (parallel)

Implement `handle_selection_range`. No dependency on Step 1; can start
immediately alongside 2A, 2C, 2D.

**TDD cycle:**

1. Write all unit tests below; stub `handle_selection_range`.
2. `cargo test` — new tests fail.
3. Implement; then `cargo clippy -- -D warnings`.

**Deliverables:**

- Add to `use lsp_types::{...}`:
  `SelectionRange, SelectionRangeParams`
- Add `pub(crate) fn handle_selection_range(params: SelectionRangeParams, index: &NoteIndex) -> Vec<SelectionRange>` to `src/handlers.rs`
- Private helpers:
  - `fn word_range_at(line: &str, cursor_char: u32) -> Option<Range>` — returns the UTF-16 range of the word under the cursor
  - `fn paragraph_range(content: &str, line: u32) -> Range` — scans backward/forward from `line` to the nearest blank lines

**Unit tests:**

| Test                                        | What it verifies                                                                |
| ------------------------------------------- | ------------------------------------------------------------------------------- |
| `selection_range_word_at_cursor`            | cursor in the middle of a word returns that word's range                        |
| `selection_range_no_word_on_whitespace`     | cursor on whitespace omits the word level                                       |
| `selection_range_cursor_inside_link`        | cursor inside a Markdown link's text span includes link range as parent of word |
| `selection_range_paragraph_bounds`          | paragraph range spans from first to last non-blank line of the paragraph        |
| `selection_range_section_range`             | cursor in a heading section body includes the section range above paragraph     |
| `selection_range_document_always_outermost` | outermost range is always the full document                                     |
| `selection_range_multiple_positions`        | result vec has the same length as `params.positions`                            |

> **Manual checkpoint:** No editor action yet — run `cargo test` and confirm all
> 7 new tests pass.

---

## Step 2C — Inlay hints (parallel)

Implement `handle_inlay_hints`. No dependency on Step 1; can start immediately.

**TDD cycle:**

1. Write all unit tests below; stub `handle_inlay_hints`.
2. `cargo test` — new tests fail.
3. Implement; then `cargo clippy -- -D warnings`.

**Deliverables:**

- Add to `use lsp_types::{...}`:
  `InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams`
- Add `pub(crate) fn handle_inlay_hints(params: InlayHintParams, index: &NoteIndex) -> Vec<InlayHint>` to `src/handlers.rs`

**Unit tests:**

| Test                                  | What it verifies                                                                 |
| ------------------------------------- | -------------------------------------------------------------------------------- |
| `inlay_hint_shows_title`              | link to a note with `title:` frontmatter produces one hint at `target_range.end` |
| `inlay_hint_label_is_title_string`    | hint `label` equals the target note's title string                               |
| `inlay_hint_omits_note_without_title` | link to a note without `title:` produces no hint                                 |
| `inlay_hint_omits_broken_link`        | link to a missing file produces no hint                                          |
| `inlay_hint_omits_url`                | external URL link produces no hint                                               |
| `inlay_hint_filtered_by_range`        | hint whose position falls outside `params.range` is excluded                     |

> **Manual checkpoint:** No editor action yet — run `cargo test` and confirm all
> 6 new tests pass.

---

## Step 2D — Code lens heading lenses (parallel)

Extend `handle_code_lens` to add per-heading anchor-link lenses. No dependency
on Step 1; can run in parallel with 2A–2C.

**Note:** This step modifies an existing function rather than adding a new one.
The conflict risk at merge time is low (a single function in `src/handlers.rs`)
but the merging agent should verify the full function body is intact.

**TDD cycle:**

1. Write all unit tests below first.
2. `cargo test` — new tests fail.
3. Extend `handle_code_lens`; then `cargo clippy -- -D warnings`.

**Deliverables:**

- Extend `pub(crate) fn handle_code_lens` in `src/handlers.rs` to append
  per-heading `CodeLens` entries after the existing backlinks lens block
- No new imports required beyond what is already in scope

**Unit tests:**

| Test                                     | What it verifies                                                             |
| ---------------------------------------- | ---------------------------------------------------------------------------- |
| `code_lens_heading_same_file_anchors`    | bare anchor `[text](#slug)` in same file counted toward heading lens         |
| `code_lens_heading_cross_file_anchors`   | `[text](path.md#slug)` from another note counted toward heading lens         |
| `code_lens_heading_no_anchors_no_lens`   | heading with no incoming anchor links produces no heading lens               |
| `code_lens_heading_lens_at_heading_line` | heading lens `range.start` equals the heading's `range.start`                |
| `code_lens_backlinks_lens_still_present` | original backlinks lens at line 0 is still returned alongside heading lenses |

> **Manual checkpoint:** No editor action yet — run `cargo test` and confirm all
> 5 new tests pass and the existing `code_lens_*` tests are unaffected.

---

## Step 3 — Wire capabilities and dispatch (serial merge)

After all parallel steps are merged to main, wire the four new handlers into
the protocol handler and resolve any import conflicts. This step is the only
one that touches `src/server/mod.rs`.

**Deliverables:**

- Resolve any `use lsp_types::{...}` import conflicts in `src/handlers.rs`
  (additive only — no logic conflicts expected)
- Add to `src/server/mod.rs` `use lsp_types::{...}`:
  `FoldingRangeParams, FoldingRangeProviderCapability, InlayHintParams,
InlayHintServerCapabilities, SelectionRangeParams, SelectionRangeProviderCapability`
- Extend `ServerCapabilities` in `run()`:
  ```rust
  folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
  selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
  inlay_hint_provider: Some(OneOf::Left(true)),
  ```
- Add three dispatch arms to `dispatch_request()`:
  - `"textDocument/foldingRange"` → `handle_folding_ranges`
  - `"textDocument/selectionRange"` → `handle_selection_range`
  - `"textDocument/inlayHint"` → `handle_inlay_hints`
- Run `cargo test` and `cargo clippy -- -D warnings`

**Unit tests:** None new — Step 3 is pure wiring. All handler behaviour is
covered by the unit tests from Steps 2A–2D.

> **Manual checkpoint:** Open a vault in VS Code or Zed. Verify that each new
> capability appears in the server's `initialize` response by checking the LSP
> log. Confirm that folding markers appear on heading lines and code fences, and
> that no previously passing feature is broken.

---

## Step 4 — Integration tests (serial)

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                           | What it verifies                                                           |
| ------------------------------ | -------------------------------------------------------------------------- |
| `folding_ranges_round_trip`    | full session returns fold regions for headings and fenced blocks           |
| `selection_range_round_trip`   | full session returns a correct selection chain for a given cursor position |
| `inlay_hints_round_trip`       | full session returns hints for links whose targets have a `title:`         |
| `code_lens_heading_round_trip` | full session returns heading lenses alongside the existing backlinks lens  |

> **Manual checkpoint (full session):** Open a vault with multiple notes,
> headings, fenced code blocks, and cross-file links. Walk each feature:
> heading sections fold, selection expand works, linked-note titles appear as
> inlay hints, and anchor-link counts appear on headings. Confirm that backlinks
> lens, link completions, diagnostics, and rename from earlier releases are
> unaffected.

---

## Done — v0.9 complete

| Story | Feature                                               | Delivered in step |
| ----- | ----------------------------------------------------- | ----------------- |
| US-36 | Folding ranges for heading sections and fenced blocks | Step 1 + 2A       |
| US-52 | Selection range — smart expand/contract chain         | Step 2B           |
| US-53 | Inlay hints — linked-note title inline                | Step 2C           |
| US-54 | Code lens on headings — anchor link count             | Step 2D           |
