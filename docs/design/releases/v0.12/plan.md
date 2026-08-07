# v0.12 Implementation Plan — Headless Rename

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the CLI should be manually verified.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                  | Status | Notes |
| ----------------------------------------------------- | ------ | ----- |
| 1 — `LineIndex::offset`                               | Todo   |       |
| 2 — Extract `compute_heading_rename` + `find_heading` | Todo   |       |
| 3 — Extract `compute_tag_rename`                      | Todo   |       |
| 4 — `edit::apply` (Edit Applicator)                   | Todo   |       |
| 5 — `knap rename-file`                                | Todo   |       |
| 6 — `knap rename-heading`                             | Todo   |       |
| 7 — `knap rename-tag`                                 | Todo   |       |
| 8 — Integration tests + docs                          | Todo   |       |

---

## Step 1 — `LineIndex::offset`

The one new primitive everything else in this release needs: converting an
LSP `Position` back to a byte offset, so a `TextEdit` computed by a handler
can be applied to a file's raw text. Comes first because every later step
that writes to disk depends on it.

**Deliverables:**

- `LineIndex::offset(&self, position: Position) -> usize` in
  `src/parser/mod.rs`, next to the existing `position()`/`range()`.

Write the unit tests first — stub `offset` to `unimplemented!()` so the file
compiles, confirm the tests fail, then implement until green.

**Unit tests:**

| Test                                | What it verifies                                                                                                |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `offset_round_trips_with_position`  | `line_index.offset(line_index.position(n)) == n` across several byte offsets, including mid-line and line-start |
| `offset_handles_multibyte_line`     | A line containing a surrogate-pair character (emoji) offsets correctly on both sides of it                      |
| `offset_clamps_past_end_of_content` | A `Position` beyond the last line returns `content.len()`, no panic                                             |

> **Manual checkpoint:** none — `LineIndex` has no CLI surface yet. Verified
> purely by `cargo test`.

---

## Step 2 — Extract `compute_heading_rename` + `find_heading`

Pulls the heading-rename edit computation out of the cursor-driven
`handle_rename` into a position-independent function, and adds the
text-or-slug lookup the CLI needs to turn `<old>` into a `Heading`. The LSP
path's behavior must not change — this step is a refactor with a regression
net, not new behavior.

**Deliverables:**

- `src/handlers.rs`: add `find_heading<'a>(note: &'a parser::Note, query: &str)
-> Option<&'a parser::Heading>` (exact-text match, case-insensitive, then
  slug match).
- Add `compute_heading_rename(path: &Path, note: &parser::Note, heading:
&parser::Heading, new_name: &str, index: &NoteIndex) -> WorkspaceEdit`,
  moving today's heading branch (`handle_rename`, lines ~1195–1254) into it
  verbatim.
- `handle_rename`'s heading branch shrinks to: locate the heading at the
  cursor position (existing lookup, unchanged), then call
  `compute_heading_rename`.

Write the new unit tests first against the extracted functions; confirm
they fail to compile/fail until `compute_heading_rename`/`find_heading`
exist and work, then implement.

**Unit tests:**

| Test                                                | What it verifies                                                  |
| --------------------------------------------------- | ----------------------------------------------------------------- |
| `find_heading_matches_exact_text`                   | Case-insensitive text match                                       |
| `find_heading_matches_slug`                         | A query equal to the heading's GFM slug matches                   |
| `find_heading_no_match_returns_none`                | Neither text nor slug matches → `None`                            |
| `compute_heading_rename_updates_same_file_anchors`  | Bare `#slug` self-link in the same file rewritten to the new slug |
| `compute_heading_rename_updates_cross_file_anchors` | `[text](note.md#old-slug)` in another file rewritten              |

Existing `handle_rename` heading tests in `src/handlers.rs` must stay green
unmodified — this proves the extraction didn't change LSP behavior.

> **Manual checkpoint:** In an editor connected to `knap lsp`, rename a
> heading via the rename dialog exactly as before this step; result is
> unchanged (this step is a pure refactor of the LSP path).

---

## Step 3 — Extract `compute_tag_rename`

Same pattern as Step 2, for tags.

**Deliverables:**

- `src/handlers.rs`: add `compute_tag_rename(old_name: &str, new_name: &str,
index: &NoteIndex) -> WorkspaceEdit`, iterating every note in
  `index.notes_by_tag(old_name)` and emitting an edit for each occurrence —
  no "current file" special case (that case only existed for a cursor's
  unindexed buffer, which doesn't apply here).
- `handle_rename`'s tag branch keeps its own disk-fallback handling for the
  current file (unchanged, since an LSP rename can still target an unindexed
  buffer), then calls `compute_tag_rename` for every other note instead of
  hand-rolling the same loop inline.

**Unit tests:**

| Test                                             | What it verifies                                                                                                 |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `compute_tag_rename_covers_all_notes`            | Every note carrying the tag gets an edit, across all three YAML tag forms (bare scalar, inline list, block list) |
| `compute_tag_rename_no_notes_returns_empty_edit` | A tag no note carries → `WorkspaceEdit` with empty `changes`, not a panic                                        |

Existing `handle_rename` tag tests must stay green unmodified.

> **Manual checkpoint:** In an editor connected to `knap lsp`, rename a tag
> via the rename dialog exactly as before this step; result is unchanged.

---

## Step 4 — `edit::apply` (Edit Applicator)

The piece with no LSP precedent: executing a computed `WorkspaceEdit`
against files on disk. Lands as its own top-level module, `src/edit.rs`, per
the Edit Applicator component in `docs/ARCHITECTURE.md` — not inside
`src/cli/`, because it's not rename-specific: v0.13's `knap fix` needs the
same capability against `WorkspaceEdit`s handlers already emit today (the
"create missing file" quick fix's `Op(ResourceOp::Create)` shape).

**Deliverables:**

- New `src/edit.rs`, `pub mod edit;` in `src/lib.rs`. `pub(crate) fn
apply(edit: &lsp_types::WorkspaceEdit) -> anyhow::Result<usize>`, two
  phases:
  1. `edit.changes` — for each `(uri, edits)`: `handlers::uri_to_path`, read
     the file (`?` on failure — no silent skip), sort `edits` by descending
     `(line, character)` of `range.start`, apply each using
     `LineIndex::offset` and `String::replace_range`, write back.
  2. `edit.document_changes`, if present — walk the `Vec` in order; each
     `DocumentChangeOperation::Edit` applies like phase 1 for that one file;
     each `Op(ResourceOp::Rename { old_uri, new_uri, .. })` does
     `std::fs::rename`; each `Op(ResourceOp::Create { uri, options, .. })`
     creates an empty file, honoring `ignore_if_exists`; each
     `Op(ResourceOp::Delete { .. })` is an exhaustive match arm returning
     `anyhow::bail!("delete not supported")` — no handler emits it yet, so
     this is a deliberate not-yet-implemented error, not a silent no-op, and
     keeping the match exhaustive means a future `Delete`-emitting handler
     fails to compile here until this arm is implemented.

  Returns the total count of files touched across both phases.

Write the unit tests first (build `WorkspaceEdit`s by hand against
`tempfile`-backed fixture files; confirm they fail before `apply` exists),
then implement.

**Unit tests:**

| Test                                                   | What it verifies                                                                                                                            |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `apply_changes_single_edit`                            | One `TextEdit` in one file lands at the right byte range                                                                                    |
| `apply_changes_multiple_edits_same_file`               | Two non-overlapping edits in one file both land correctly when applied out of source order (proves descending-order application is correct) |
| `apply_changes_multiple_files`                         | Edits split across two files both get written                                                                                               |
| `apply_changes_missing_file_errors`                    | `Err`, not a silent skip, when a target file doesn't exist                                                                                  |
| `apply_document_changes_edit_then_rename_in_order`     | An `[Edit, Op(Rename)]` sequence applies the edit (to the _old_ path) before the file moves                                                 |
| `apply_document_changes_create`                        | `Op(ResourceOp::Create)` creates an empty file; existing file left alone when `ignore_if_exists` is set                                     |
| `apply_document_changes_delete_errors_not_implemented` | `Op(ResourceOp::Delete)` returns an explicit error rather than silently doing nothing                                                       |

> **Manual checkpoint:** none — no CLI surface yet. Verified purely by
> `cargo test` against `tempfile` fixtures.

---

## Step 5 — `knap rename-file`

First subcommand; wires Steps 1–4 together for the simplest case (no new
`handlers::` logic — `handle_will_rename_files` is reused as-is, wrapped
into `document_changes`).

**Deliverables:**

- `src/cli/mod.rs`: add `Commands::RenameFile { old: PathBuf, new: PathBuf }`.
- `src/cli/rename.rs`: `pub fn run_file(old: &Path, new: &Path) ->
anyhow::Result<()>` — validate `old` exists / `new` doesn't,
  `config::for_path` + `index::build`, build `RenameFilesParams`, call
  `handlers::handle_will_rename_files`, then wrap its `changes`-shaped
  result into `document_changes`: convert each `(uri, Vec<TextEdit>)` into a
  `DocumentChangeOperation::Edit(TextDocumentEdit)` (mirrors the
  construction `handle_code_actions` already uses at
  `src/handlers.rs:1296`), then push a trailing
  `DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile { old_uri,
new_uri, options: None, annotation_id: None }))`. Hand the wrapped edit to
  `edit::apply`, then print a summary line.

**Unit tests:**

| Test                                      | What it verifies                                                                                                                                                         |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `wrap_rename_file_edit_appends_rename_op` | `handle_will_rename_files`'s output, wrapped, produces `document_changes` ending in `Op(ResourceOp::Rename)`, with every incoming/outgoing edit preceding it in the list |

> **Manual checkpoint:** In a scratch vault, `knap rename-file
notes/old.md notes/new.md` where another note links to `old.md`; confirm
> the other note's link now points at `new.md` and `old.md` no longer exists
> on disk.

---

## Step 6 — `knap rename-heading`

**Deliverables:**

- `src/cli/mod.rs`: add `Commands::RenameHeading { file: PathBuf, old:
String, new: String }`.
- `src/cli/rename.rs`: `pub fn run_heading(file: &Path, old: &str, new: &str)
-> anyhow::Result<()>` — `config::for_path` + `index::build`, look up the
  note at `file` (index, falling back to a disk parse — same pattern as
  `handle_prepare_rename`), `handlers::find_heading` (bail loudly on
  `None`), `handlers::compute_heading_rename`, `edit::apply` directly (its
  `changes`-shaped output needs no wrapping — no resource op involved),
  print a summary line.

**Unit tests:**

No new pure-logic unit tests this step (covered by Step 2's
`find_heading`/`compute_heading_rename` tests) — covered by the integration
test in Step 8.

> **Manual checkpoint:** In a scratch vault, `knap rename-heading notes/a.md
"Old Section" "New Section"` where `notes/b.md` links to
> `a.md#old-section`; confirm both the heading text in `a.md` and the anchor
> link in `b.md` are updated.

---

## Step 7 — `knap rename-tag`

**Deliverables:**

- `src/cli/mod.rs`: add `Commands::RenameTag { old: String, new: String }`.
- `src/cli/rename.rs`: `pub fn run_tag(old: &str, new: &str) ->
anyhow::Result<()>` — `config::for_path(".")` + `index::build`, bail
  loudly if `index.notes_by_tag(old)` is empty, `handlers::compute_tag_rename`,
  `edit::apply` directly, print a summary line.

**Unit tests:**

No new pure-logic unit tests this step (covered by Step 3's
`compute_tag_rename` tests) — covered by the integration test in Step 8.

> **Manual checkpoint:** In a scratch vault, `knap rename-tag draft
published` across two notes using the tag in different YAML forms (inline
> list vs. block list); confirm both are updated.

---

## Step 8 — Integration tests + docs

End-to-end tests over the real binary, and doc updates — always last.

**Deliverables:**

- `tests/cli.rs` (extending v0.11's suite): all integration tests below,
  using on-disk fixtures under `tests/fixtures/` (mutated by the test, so
  each test copies its fixture into a fresh `tempfile` dir first rather than
  mutating the checked-in copy).
- `cargo test` passes, `cargo clippy -- -D warnings` clean.
- `README.md`: new subsection under "Linter (`knap lint`)"/"Indexer (`knap
index`)" documenting the three `rename-*` subcommands and their usage
  strings, matching the existing style.
- `docs/ARCHITECTURE.md`: CLI subcommand table gets three new rows
  (`rename-file`, `rename-heading`, `rename-tag`) — the Edit Applicator
  component and the updated invariant were already added when this release
  was scoped; confirm both still match what got built (e.g. if `Delete`
  gained real support instead of the not-yet-implemented error, update the
  component's description).
- `docs/USER_STORIES.md`/`docs/ROADMAP.md`: already updated as part of
  scoping this release — confirm no drift crept in during implementation.

| Test                                         | What it verifies                                                                                                       |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `rename_file_updates_incoming_and_outgoing`  | `knap rename-file` on a fixture with a linker and a link out of the moved file — both rewritten, file physically moved |
| `rename_file_new_path_exists_errors`         | Non-zero exit, no filesystem changes, when `<new>` already exists                                                      |
| `rename_heading_updates_same_and_cross_file` | Same-file bare anchor and cross-file anchor both rewritten                                                             |
| `rename_heading_accepts_slug_or_text`        | Same rename succeeds whether `<old>` is passed as heading text or its slug                                             |
| `rename_heading_not_found_errors`            | Non-zero exit, no filesystem changes, for an unmatched `<old>`                                                         |
| `rename_tag_updates_all_frontmatter_forms`   | Bare scalar, inline list, and block list tag forms across multiple files all updated                                   |
| `rename_tag_not_used_errors`                 | Non-zero exit, no filesystem changes, when no note carries `<old>`                                                     |
| `rename_respects_knap_toml_extensions`       | A non-default-extension note (via a `knap.toml` fixture) is picked up as a rename target                               |

> **Manual checkpoint (full session):** In a scratch vault with a real
> editor connected via `knap lsp`, perform the same three renames both
> through the editor's rename UI and through the equivalent `knap rename-*`
> command on a copy of the vault; diff the two results and confirm they're
> identical. Confirm earlier releases (`lint`, `index`, editor rename) are
> unaffected.

---

## Done — v0.12 complete

| Story  | Feature                                  | Delivered in step |
| ------ | ---------------------------------------- | ----------------- |
| US-D08 | `knap rename-file <old> <new>`           | Step 5            |
| US-D09 | `knap rename-heading <file> <old> <new>` | Step 6            |
| US-D10 | `knap rename-tag <old> <new>`            | Step 7            |
