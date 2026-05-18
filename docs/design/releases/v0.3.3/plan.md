# v0.3.3 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoint where the server is manually verified against a real editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                          | Status | Notes |
| ----------------------------- | ------ | ----- |
| 1 — Disk fallback in handlers | Done   |       |
| 2 — Integration test          | Done   |       |

---

## Step 1 — Disk fallback in handlers

Add `use crate::parser;` to `src/handlers.rs`. Replace the early-exit
`index.get_note(&path)?` in both rename handlers with a two-branch lookup that
falls back to `std::fs::read_to_string` + `parser::parse` when the file is
absent from the index.

**Deliverables:**

- `src/handlers.rs`: add `use crate::parser;` import
- `src/handlers.rs`: `handle_prepare_rename` — two-branch Note lookup
- `src/handlers.rs`: `handle_rename` — two-branch Note lookup
- Four new unit tests (disk I/O via `std::env::temp_dir()`):
  - `prepare_rename_disk_fallback`
  - `prepare_rename_disk_fallback_off_heading`
  - `rename_disk_fallback_edits_heading`
  - `rename_disk_fallback_no_incoming_links`

**Unit tests:**

| Test                                       | What it verifies                                                                          |
| ------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `prepare_rename_disk_fallback`             | Empty index + real file → `Some(RangeWithPlaceholder)` with correct range and placeholder |
| `prepare_rename_disk_fallback_off_heading` | Empty index + real file, cursor on a prose line → `None`                                  |
| `rename_disk_fallback_edits_heading`       | Empty index + real file → workspace edit rewrites heading text                            |
| `rename_disk_fallback_no_incoming_links`   | Empty index + real file → edit contains only the file itself (no incoming-link entries)   |

> **Manual checkpoint:** Open `docs/ROADMAP.md` in the editor (without
> restarting the server so the file is in the index from startup). Place the
> cursor on any heading. Trigger rename (`F2`). The dialog should appear
> pre-filled with the heading text, and confirming should update the heading and
> any anchors referencing it. This verifies the indexed path is unaffected.

---

## Step 2 — Integration test

End-to-end test over the full LSP message loop that reproduces the original
failure: server starts with no workspace folders (empty index), a file is
created on disk, and `prepareRename` is called without a preceding `didOpen`.

**Deliverables:**

- `tests/rename.rs` (new file, or append to existing integration test file):
  `prepare_rename_without_did_open` — verifies that `prepareRename` returns a
  non-null result for a file that was never sent via `didOpen`.
- `cargo test` passes, `cargo clippy -- -D warnings` clean.

| Test                              | What it verifies                                                                                                        |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `prepare_rename_without_did_open` | Server with empty index receives `prepareRename` for an on-disk `.md` file → non-null response with correct placeholder |

> **Manual checkpoint (full session):** Open the knap repo in an editor that
> uses the LSP server. Open `docs/ROADMAP.md`. Trigger rename on a heading.
> Confirm the dialog appears. Confirm rename updates the heading text. Open a
> note in the `notes/` directory (if one exists) that links to a ROADMAP heading
> via an anchor; confirm that anchor slug is also updated. Verify that existing
> features (completion, go-to-definition, diagnostics) are unaffected.

---

## Done — v0.3.3 complete

| Story | Feature                                      | Delivered in step |
| ----- | -------------------------------------------- | ----------------- |
| #2    | Rename works for files absent from NoteIndex | Step 1            |
