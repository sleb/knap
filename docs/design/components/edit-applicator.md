# Edit Applicator

The headless, in-process counterpart to what an LSP client does upon
`workspace/applyEdit`: given an already-computed `WorkspaceEdit`, mutate real
files on disk to match it. Lives in `src/edit.rs`, a top-level module rather
than something under `src/cli/` — it isn't rename-specific, and any handler
that emits a `WorkspaceEdit` (e.g. the "create missing file" quick fix) can
be applied here.

Introduced in v0.12 (see
[`releases/v0.12/design.md`](../releases/v0.12/design.md) § Edit Applicator
for the full rationale). This doc tracks the living contract; the release
doc is the historical record of why it exists.

---

## Dependencies

```toml
lsp-types = "0.97"
anyhow    = "1"
```

Plus `crate::handlers::uri_to_path` (URI → filesystem path) and
`crate::parser::LineIndex::offset` (LSP `Position` → byte offset), both
pre-existing.

---

## Contract

```rust
pub(crate) fn apply(edit: &lsp_types::WorkspaceEdit) -> anyhow::Result<usize>
```

Returns the number of files touched across both `changes` and
`document_changes`. A missing/unreadable file, a non-`file` URI, or a failed
resource operation is a hard error, propagated via `?` — never a silent
skip, per `AGENTS.md`.

---

## Execution

Two phases, matching the two ways a `WorkspaceEdit` carries changes. Both
run if both are present; `document_changes` runs second.

### 1. `edit.changes`

`HashMap<Uri, Vec<TextEdit>>`, unordered across files by construction. For
each file:

1. Read the file's full content
2. Sort that file's edits by **descending** `(line, character)` of
   `range.start` (`Reverse` ordering) — applying back-to-front means an
   earlier edit's byte range is never shifted by a later one landing first
3. For each edit, convert `range.start`/`range.end` to byte offsets via
   `LineIndex::offset` (recomputed per edit, since content changes after
   each `replace_range`) and splice in `new_text`
4. Write the result back

### 2. `edit.document_changes`

`Some(DocumentChanges::Operations(Vec<DocumentChangeOperation>))`, executed
**in list order** (not sorted — order is significant, see below):

| Operation                | Effect                                                                                                                                                                                                         |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Edit(TextDocumentEdit)` | Same per-file application as phase 1, for that one file                                                                                                                                                        |
| `Op(ResourceOp::Rename)` | `std::fs::rename(old_path, new_path)`                                                                                                                                                                          |
| `Op(ResourceOp::Create)` | Creates an empty file at `path`, unless it already exists and `ignore_if_exists` is set                                                                                                                        |
| `Op(ResourceOp::Delete)` | **Not implemented.** `anyhow::bail!` — fails loudly rather than a silent no-op, so a future handler that starts emitting `Delete` gets a clear implement-this-arm error instead of a quietly-dropped operation |

**Phase ordering matters**: phase 1 (`changes`) runs before phase 2
(`document_changes`) so a `rename-file`-style sequence — edit the file at
its old path, then move it — lands correctly: the edit's target is the
file's _old_ path, so it must apply before the rename, not after.

---

## Callers

None yet outside its own unit tests — `apply` currently carries
`#[allow(dead_code)]`. Its first caller is `knap rename-file` (v0.12); the
other v0.12 subcommands (`rename-heading`, `rename-tag`) and v0.13's `knap
fix` are also headless CLI commands expected to call it. `handlers.rs`
never calls it, and neither does `knap lsp` — when a real editor is
connected, the editor applies its own edits over `workspace/applyEdit`, and
this module doesn't run. See `docs/ARCHITECTURE.md` § Boundaries and
Invariants.

---

## Testing

Unit tests live in `src/edit.rs` itself (`#[cfg(test)] mod tests`), each
building a `WorkspaceEdit` by hand and asserting on real files in a
`tempdir()`:

| Test                                                   | What it verifies                                                                      |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| `apply_changes_single_edit`                            | One edit in one file lands correctly                                                  |
| `apply_changes_multiple_edits_same_file`               | Two non-overlapping edits in one file, listed out of source order, both land          |
| `apply_changes_multiple_files`                         | Edits across multiple files in `changes` are all applied; return count matches        |
| `apply_changes_missing_file_errors`                    | Missing file → `Err`, not a silent skip                                               |
| `apply_document_changes_edit_then_rename_in_order`     | `[Edit, Op(Rename)]` applies the edit before the move — edit targets the _old_ path   |
| `apply_document_changes_create`                        | `Op(Create)` creates an empty file; `ignore_if_exists` respected when already present |
| `apply_document_changes_delete_errors_not_implemented` | `Op(Delete)` → `Err`, file untouched                                                  |
