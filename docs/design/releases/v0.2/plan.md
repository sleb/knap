# v0.2 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                     | Status | Notes                     |
| ------------------------ | ------ | ------------------------- |
| 1 — Broaden file watcher | Done   |                           |
| 2 — Non-note completions | Done   |                           |
| 3 — Rename handler       | Done   | TDD (tests written first) |
| 4 — Integration tests    | Done   | TDD (tests written first) |

## Approach

Starting from Step 3, all new logic follows TDD:

1. Write all unit tests for the deliverable first — stub the function signature if needed to compile
2. Run `cargo test` and confirm the tests **fail** before writing any implementation
3. Implement until all tests pass, then run `cargo clippy -- -D warnings`

Step 4 must follow the same cycle: write the integration tests, confirm they fail, then make them pass.

---

## Step 1 — Broaden file watcher

Remove `attachments_dir` from config and replace the per-extension + optional
directory watchers with a single `**/*` watcher per workspace root. This makes
runtime attachment tracking work without any user configuration, delivering
US-26 and making US-21 fully operational at runtime.

**Deliverables:**

- Remove `attachments_dir` field from `Config` and `InitOptions` in
  `src/server/mod.rs`
- Replace the `register_file_watcher` body: one `FileSystemWatcher` per
  workspace root using `GlobPattern::Relative` with pattern `"**/*"`
- Add `should_skip_path(path: &Path) -> bool` in `src/server/mod.rs`; reuse
  the same name-based logic as `should_skip_dir` in the index crawler
  (hidden dirs, `node_modules`, `target`)
- Call `should_skip_path` at the top of the per-event loop in
  `on_did_change_watched_files`; skip the event if it returns `true`

**Unit tests:**

_No new unit tests — the watcher change is covered by the integration tests in
Step 4._

> **Manual checkpoint:** Start the server against a vault. Add a new image file
> (`touch vault/img.png`) outside any configured directory. Confirm that a note
> containing `![alt](img.png)` loses its broken-link diagnostic. Delete the file
> and confirm the diagnostic returns. Verify notes under `.git/` or `target/`
> do not trigger index updates.

---

## Step 2 — Non-note completions

Expose attachment paths from the index and include them in path completions,
delivering US-44.

**Deliverables:**

- Add `pub fn all_attachment_paths(&self) -> impl Iterator<Item = &PathBuf>`
  to `NoteIndex` in `src/index/mod.rs`: filters `all_files` to paths not
  present in `by_path`
- Update `handle_completion` in `src/handlers.rs`: chain a second iterator
  over `index.all_attachment_paths()` after the notes iterator; label is
  `path.file_name()` as a string, `insert_text` and `filter_text` are the
  relative path from `from_dir`, kind is `FILE`

**Unit tests:**

| Test                                      | What it verifies                                               |
| ----------------------------------------- | -------------------------------------------------------------- |
| `all_attachment_paths_excludes_notes`     | Only non-note files returned; note paths absent                |
| `completion_includes_attachments`         | Attachment paths appear in items when trigger fires            |
| `completion_attachment_label_is_filename` | Label is the bare filename; `insert_text` is the relative path |

> **Manual checkpoint:** Open a vault containing a mix of `.md` notes and
> `.png` / `.pdf` files. Type `[link](` in a note and confirm the completion
> list includes both note paths and attachment paths. Verify the insert text is
> a correct relative path from the current file.

---

## Step 3 — Rename handler

Implement `workspace/willRenameFiles` to update all incoming and outgoing links
atomically, delivering US-04.

**Deliverables:**

- Add `handle_will_rename_files(params: RenameFilesParams, index: &NoteIndex) -> WorkspaceEdit`
  to `src/handlers.rs`
  - For each `FileRename { old_uri, new_uri }` in `params.files`:
    - **Incoming**: iterate `index.links_to(&old_path)`; for each `LocatedLink`
      compute `new_target = relative_path(source_dir, &new_path)` and push a
      `TextEdit` on `located.md_link.target_range` into the source file's entry
      in `changes`
    - **Outgoing**: iterate `index.get_note(&old_path).md_links`; skip empty
      targets and URLs; compute `abs_target = normalize_path(old_dir.join(&link.target))`,
      then `new_target = relative_path(new_dir, &abs_target)`; push a `TextEdit`
      on `link.target_range` into `old_path`'s entry only when `new_target != link.target`
  - Return `WorkspaceEdit { changes: Some(changes), ..Default::default() }`
- Update `ServerCapabilities` in `src/server/mod.rs`: add `workspace` field
  with `file_operations.will_rename` advertising a `"**/*"` filter
- Add `"workspace/willRenameFiles"` arm to `dispatch_request`; deserialize
  `RenameFilesParams`, call `handle_will_rename_files`, respond with the edit
- Add required imports: `lsp_types::{RenameFilesParams, TextEdit, WorkspaceEdit,
WorkspaceServerCapabilities, ServerCapabilitiesFileOperations,
FileOperationRegistrationOptions, FileOperationFilter, FileOperationPattern}`

**Unit tests:**

| Test                                        | What it verifies                                                           |
| ------------------------------------------- | -------------------------------------------------------------------------- |
| `rename_updates_incoming_links`             | Incoming `target_range` rewritten with path relative to the source file    |
| `rename_updates_outgoing_links`             | Links inside the renamed note rewritten relative to the new base directory |
| `rename_updates_both_incoming_and_outgoing` | A note linking to and linked from `old_path` receives both edit groups     |
| `rename_skips_url_targets`                  | External URL links are not emitted as edits                                |
| `rename_no_changes_same_dir`                | Rename within the same directory leaves same-dir relative paths unchanged  |
| `rename_unlinked_file_empty_edit`           | Renaming a file with no links returns an empty `WorkspaceEdit`             |

> **Manual checkpoint:** In an editor connected to the server, rename a note
> that is linked from two other notes. Confirm all incoming links are updated to
> the new path. Also confirm that outgoing links inside the renamed file are
> updated to be correct from the new location. Verify that renaming a file with
> no links produces no spurious edits.

---

## Step 4 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- `tests/lsp.rs` additions — all integration tests listed below
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                      | What it verifies                                                          |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| `test_will_rename_incoming`               | `workspace/willRenameFiles` returns correct edits for incoming links      |
| `test_will_rename_outgoing`               | `workspace/willRenameFiles` returns correct edits for outgoing links      |
| `test_will_rename_no_links`               | Renaming an unlinked file returns an empty `WorkspaceEdit`                |
| `test_attachment_no_diagnostic`           | Link to a non-Markdown workspace file produces no diagnostic              |
| `test_attachment_added_clears_diagnostic` | `didChangeWatchedFiles` Created for attachment clears broken-link warning |
| `test_attachment_deleted_adds_diagnostic` | `didChangeWatchedFiles` Deleted for attachment introduces a new warning   |
| `test_completion_includes_attachment`     | Completion at `](` returns items for non-Markdown workspace files         |

> **Manual checkpoint (full session):** Open a real vault. Walk the complete
> golden path for this release: (1) type `[link](` and confirm both notes and
> attachments appear in completions; (2) create a link to an image and confirm
> no broken-link diagnostic; (3) delete the image and confirm the diagnostic
> appears; (4) rename a note that is linked from two files and confirm both
> incoming links update and the renamed file's outgoing links update. Confirm
> all v0.1 capabilities (Go to Definition, Find References, broken-link
> diagnostics for missing notes) are unaffected.

---

## Done — v0.2 complete

| Story | Feature                                          | Delivered in step |
| ----- | ------------------------------------------------ | ----------------- |
| US-26 | Attachment links resolve cleanly                 | Step 1            |
| US-21 | `extensions` config fully operational at runtime | Step 1            |
| US-44 | Path completions include non-Markdown files      | Step 2            |
| US-04 | Rename file → all Markdown links updated         | Step 3            |
