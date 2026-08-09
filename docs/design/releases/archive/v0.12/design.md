# v0.12 Design — Headless Rename

Covers the stories in the v0.12 release:

| Story  | Feature                                                                                    |
| ------ | ------------------------------------------------------------------------------------------ |
| US-D08 | `knap rename-file <old> <new>` — move a note, rewrite incoming + outgoing links atomically |
| US-D09 | `knap rename-heading <file> <old> <new>` — rewrite heading text + all anchor links         |
| US-D10 | `knap rename-tag <old> <new>` — rewrite every frontmatter occurrence across the workspace  |

---

## Goal

v0.11 gave agents read access to knap's engine (`lint`, `index --json`). This
release gives them write access to the one thing that was still LSP-only:
atomic, multi-file rename. An agent restructuring a workspace today has to
either drive a full LSP session just to get `willRenameFiles`/`rename`, or
hand-roll its own find-and-replace across every linking file — slower than
knap's own index-backed resolution and one broken-anchor-regex away from
silently corrupting a link. `knap rename-file`/`rename-heading`/`rename-tag`
close that gap: the same edit computation the editor's rename UI uses,
invoked directly with an explicit target instead of a cursor position, and
applied straight to disk.

All three subcommands reuse the existing `handlers::` edit-computation logic
rather than reimplementing it. `handle_will_rename_files` already takes
explicit old/new locations (no cursor involved) and needs no changes.
`handle_rename`'s heading and tag branches are cursor-driven — this release
extracts each into a position-independent function that both the existing
LSP handler and the new CLI commands call, so there is exactly one
implementation of "what edits does renaming X produce," not two that can
drift apart.

Applying a `WorkspaceEdit` to files on disk is new: per
`docs/ARCHITECTURE.md`, handlers compute edits but never apply them, so this
release adds the Edit Applicator (`src/edit.rs`) — a new top-level component,
not a CLI-internal helper — as the in-process counterpart to what an LSP
client does upon `workspace/applyEdit`. It exists as its own component,
rather than living inside `src/cli/rename.rs`, because it is not
rename-specific: v0.13's `knap fix` needs the exact same capability to apply
code actions headlessly, and already-emitted `WorkspaceEdit`s (e.g. the
"create missing file" quick fix) are shaped for it today.

---

## Parser Changes

`LineIndex` (`src/parser/mod.rs`) currently only converts a byte offset to an
LSP `Position` (`position()`/`range()`). Applying a `TextEdit` to a file's
full text requires the inverse — `Position` to byte offset — which nothing
in the codebase does today (the LSP client does this, not the server).

```rust
impl<'a> LineIndex<'a> {
    /// Inverse of `position()`: LSP `Position` → byte offset into `content`.
    pub fn offset(&self, position: Position) -> usize
}
```

Algorithm: look up `line_starts[position.line]` for the line's byte start,
slice that line out of `content`, then walk its `char_indices()` accumulating
UTF-16 code units (`char.len_utf16()`) until `position.character` is reached,
returning `line_start + <byte offset within the line>`. This is the same
UTF-16-aware walk `handlers::utf16_to_byte_offset` already does for a single
line's text — `offset()` becomes the one implementation; whether
`utf16_to_byte_offset` is deleted in favor of calling `offset()` is left to
implementation (they're equivalent, not a design decision).

Edge cases:

- `position.line` beyond the last line → clamp to `content.len()` (defensive;
  should not happen for edits computed from this file's own index, but a
  hard panic here would take down a whole rename for one bad range)
- `position.character` beyond the line's length (e.g. end-of-line position)
  → returns the line's end byte offset (existing `char_indices` walk falls
  through to it naturally)

---

## Handler Changes

### `compute_heading_rename` (new, extracted from `handle_rename`)

```rust
pub(crate) fn compute_heading_rename(
    path: &Path,
    note: &parser::Note,
    heading: &parser::Heading,
    new_name: &str,
    index: &NoteIndex,
) -> WorkspaceEdit
```

Body is `handle_rename`'s existing heading branch (lines 1195–1254 today),
unchanged — heading-text edit, same-file anchor-only self-links, cross-file
incoming anchor links — just taking `heading`/`note` as parameters instead of
resolving them from a cursor `Position`. `handle_rename` becomes: find the
heading at the cursor, then call this. No behavior change for the LSP path.

### `find_heading<'a>` (new, small helper)

```rust
pub(crate) fn find_heading<'a>(note: &'a parser::Note, query: &str) -> Option<&'a parser::Heading>
```

Matches `query` against a heading's literal text first (case-insensitive),
falling back to matching `slug(query) == slug(heading.text)`. This is what
lets `knap rename-heading <file> <old> <new>` accept either the heading's
exact text or its slug for `<old>`, per US-D09.

### `compute_tag_rename` (new, extracted from `handle_rename`)

```rust
pub(crate) fn compute_tag_rename(old_name: &str, new_name: &str, index: &NoteIndex) -> WorkspaceEdit
```

Body is `handle_rename`'s existing tag branch, generalized: iterate every
note the index has for `old_name` (`index.notes_by_tag`) and emit an edit for
each occurrence — no "current file" special case, because that case existed
only to cover a cursor's file being unindexed (a brand-new unsaved buffer),
which cannot happen for a CLI invocation working off a freshly built index.
`handle_rename`'s tag branch keeps its own disk-fallback handling for the
current (possibly unindexed) file, then calls this for every other note; the
LSP path's behavior is unchanged.

### `handle_will_rename_files` — unchanged

Already takes explicit `old_uri`/`new_uri` pairs (`RenameFilesParams`), not a
cursor position. Its contract does not change: it still only computes edits
to _other_ files' links and the moved file's own outgoing links — it must
never add a `Rename` resource op for the file itself, since a real LSP
client calling it is already about to perform that move on its own; adding
one here would double-move it. The CLI wraps its output (below) rather than
changing it.

---

## Edit Applicator

New top-level module `src/edit.rs`, per the `docs/ARCHITECTURE.md` Edit
Applicator component — the in-process counterpart to what an LSP client does
upon `workspace/applyEdit`. This is the only new filesystem-writing code in
the release; every `rename-*` subcommand ends by handing it a fully-computed
`WorkspaceEdit`.

```rust
/// Executes `edit` against real files on disk. Returns the number of files
/// touched. A missing/unreadable file, or a failed resource operation, is a
/// hard error — propagated via `?`, never a silent skip, per `AGENTS.md`.
pub(crate) fn apply(edit: &lsp_types::WorkspaceEdit) -> anyhow::Result<usize>
```

Two phases, matching the two ways a `WorkspaceEdit` carries changes:

1. **`edit.changes`** (`HashMap<Uri, Vec<TextEdit>>`, today's shape for
   `compute_heading_rename`/`compute_tag_rename`'s output) — for each file,
   apply its edits in descending `(line, character)` order (`Position`s
   converted to byte offsets via the new `LineIndex::offset`, so an earlier
   edit's byte range never shifts a later one's), write the result back.
2. **`edit.document_changes`** (`Some(DocumentChanges::Operations(Vec<...>))`,
   the shape `rename-file`'s wrapped edit uses below) — executed **in list
   order**: `DocumentChangeOperation::Edit(TextDocumentEdit)` applies like
   phase 1 for that one file; `DocumentChangeOperation::Op(ResourceOp::Rename
{ old_uri, new_uri, .. })` does `std::fs::rename`; `Op(ResourceOp::Create
{ uri, .. })` creates an empty file if absent (`ignore_if_exists`
   respected) — this variant isn't exercised by this release's rename
   commands, but `handle_code_actions`' "create missing file" quick fix
   already emits it, and v0.13's `knap fix` will be the first headless
   caller. `Op(ResourceOp::Delete { .. })` is out of scope for both this
   release and v0.13 — no handler emits it today — but the match arm is
   exhaustive rather than a wildcard: it returns `anyhow::bail!("delete not
supported")` rather than silently no-op'ing, so a future handler that
   starts emitting `Delete` fails loudly here (a clear implement-this-arm
   error) instead of a change that appears to work but quietly drops the
   operation.

Phase ordering (1 before 2) matters for `rename-file`: the edit to the file
at its old path must land before the physical move happens.

---

## CLI Changes

New module `src/cli/rename.rs` with three entry points, one per subcommand.
All three follow the same shape: resolve config → build the index → compute
a `WorkspaceEdit` via the shared `handlers::` functions above → hand it to
`edit::apply` → report what changed.

### `knap rename-file <old> <new>`

```rust
pub fn run(old: &Path, new: &Path) -> anyhow::Result<()>
```

1. `old` must exist on disk and `new` must not (no clobber) — `anyhow::bail!`
   otherwise.
2. `config::for_path(old.parent()...)` → `index::build` (same pattern as
   `lint`/`index`).
3. Build a `RenameFilesParams` with one `FileRename { old_uri, new_uri }` and
   call `handlers::handle_will_rename_files`, producing a `changes`-shaped
   `WorkspaceEdit`.
4. Wrap it into `document_changes`: convert each `(uri, Vec<TextEdit>)` from
   `changes` into a `DocumentChangeOperation::Edit(TextDocumentEdit { ... })`
   (same construction `handle_code_actions` already uses for its "create
   missing file" action, at `src/handlers.rs:1296`), then push a trailing
   `DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile { old_uri,
new_uri, options: None, annotation_id: None }))`. This ordered sequence —
   edits, then the move — is exactly the shape `edit::apply`'s phase 2
   executes in order, so the file physically moves only after every edit
   that targets its old path has landed.
5. `edit::apply(&wrapped_edit)`.
6. Report: `renamed <old> to <new>, updated N link(s) in M file(s)`.

Zero incoming/outgoing links to update is not an error — the file still
moves. (Unlike heading/tag rename below, "nothing to update" is an expected,
common case here, not a sign the target didn't exist.)

### `knap rename-heading <file> <old> <new>`

```rust
pub fn run(file: &Path, old: &str, new: &str) -> anyhow::Result<()>
```

1. `config::for_path` → `index::build`.
2. Look up the note at `file` (index, falling back to a disk parse the same
   way `handle_prepare_rename` does, so an unindexed file still works).
3. `handlers::find_heading(note, old)` — `None` is a hard error: `error:
no heading matching '<old>' in <file>` (fail loud; a typo here should
   never silently no-op).
4. `handlers::compute_heading_rename(file, note, heading, new, &index)` —
   already `changes`-shaped, no wrapping needed (no resource op involved).
5. `edit::apply`.
6. Report: `renamed heading '<old>' to '<new>', updated N link(s) in M file(s)`.

### `knap rename-tag <old> <new>`

```rust
pub fn run(old: &str, new: &str) -> anyhow::Result<()>
```

1. `config::for_path(".")` → `index::build` — tag rename is workspace-wide,
   no single target file, so the root is always the CLI's `path`/cwd
   (mirrors `lint`/`index`'s directory form).
2. `index.notes_by_tag(old)` empty → hard error: `error: no notes use tag
'<old>'` (same fail-loud rationale as heading rename).
3. `handlers::compute_tag_rename(old, new, &index)`.
4. `edit::apply`.
5. Report: `renamed tag '<old>' to '<new>', updated N occurrence(s) in M
file(s)`.

### `src/cli/mod.rs`

Three new `Commands` variants:

```rust
RenameFile { old: PathBuf, new: PathBuf },
RenameHeading { file: PathBuf, old: String, new: String },
RenameTag { old: String, new: String },
```

dispatched to `rename::run_file`/`rename::run_heading`/`rename::run_tag`
(one function per subcommand in `src/cli/rename.rs`, matching the existing
one-module-per-subcommand convention — `rename.rs` holds all three since
they share the wrap-and-apply pattern, but the actual disk writing now lives
in `edit::apply`, not here).

---

## Testing

### Unit tests

| Test (file)                                                             | What it verifies                                                                                                               |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `offset_round_trips_with_position` (`src/parser/mod.rs`)                | `line_index.offset(line_index.position(n)) == n` for various byte offsets                                                      |
| `offset_handles_multibyte_line` (`src/parser/mod.rs`)                   | UTF-16 surrogate-pair characters (e.g. emoji) offset correctly                                                                 |
| `offset_clamps_past_end_of_content` (`src/parser/mod.rs`)               | Position beyond the last line → `content.len()`, no panic                                                                      |
| `find_heading_matches_exact_text` (`src/handlers.rs`)                   | `query == heading.text` (case-insensitive) matches                                                                             |
| `find_heading_matches_slug` (`src/handlers.rs`)                         | `query` as a GFM slug matches a heading whose text slugifies to it                                                             |
| `find_heading_no_match_returns_none` (`src/handlers.rs`)                | Query matching neither text nor slug → `None`                                                                                  |
| `compute_heading_rename_updates_same_file_anchors` (`src/handlers.rs`)  | Bare `#slug` self-link in the same file is rewritten                                                                           |
| `compute_heading_rename_updates_cross_file_anchors` (`src/handlers.rs`) | `[text](note.md#old-slug)` in another file is rewritten                                                                        |
| `compute_tag_rename_covers_all_notes` (`src/handlers.rs`)               | Every note carrying the tag gets an edit, including one that would've been the "current file" under the old cursor-driven path |
| `apply_changes_multiple_edits_same_file` (`src/edit.rs`)                | Two non-overlapping edits in one file both land correctly when applied out of source order (phase 1)                           |
| `apply_changes_missing_file_errors` (`src/edit.rs`)                     | `Err`, not a silent skip, per `AGENTS.md`                                                                                      |
| `apply_document_changes_edit_then_rename_in_order` (`src/edit.rs`)      | An `[Edit, Op(Rename)]` sequence applies the edit before the file moves — the edit's target is the _old_ path                  |
| `apply_document_changes_create` (`src/edit.rs`)                         | `Op(ResourceOp::Create)` creates an empty file; `ignore_if_exists` respected when the file already exists                      |
| `wrap_rename_file_edit_appends_rename_op` (`src/cli/rename.rs`)         | `handle_will_rename_files`'s `changes`-shaped output, wrapped, produces `document_changes` ending in `Op(ResourceOp::Rename)`  |

### Integration tests (`tests/cli.rs`, extending v0.11's suite)

| Test                                         | What it verifies                                                                                                       |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `rename_file_updates_incoming_and_outgoing`  | `knap rename-file` on a fixture with a linker and a link out of the moved file — both rewritten, file physically moved |
| `rename_file_new_path_exists_errors`         | Non-zero exit, no filesystem changes, when `<new>` already exists                                                      |
| `rename_heading_updates_same_and_cross_file` | `knap rename-heading` fixture — same-file bare anchor and cross-file anchor both rewritten                             |
| `rename_heading_accepts_slug_or_text`        | Same rename succeeds whether `<old>` is passed as heading text or its slug                                             |
| `rename_heading_not_found_errors`            | Non-zero exit, no filesystem changes, for an unmatched `<old>`                                                         |
| `rename_tag_updates_all_frontmatter_forms`   | Bare scalar, inline list, and block list tag forms across multiple files all updated                                   |
| `rename_tag_not_used_errors`                 | Non-zero exit, no filesystem changes, when no note carries `<old>`                                                     |
| `rename_respects_knap_toml_extensions`       | A non-default-extension note (via `knap.toml` fixture) is picked up as a rename target                                 |
