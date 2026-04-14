# v0.2 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                         | Status  | Notes |
| ---------------------------- | ------- | ----- |
| 1 — Config                   | ✅ Done |       |
| 2 — Attachment file tracking | ✅ Done |       |
| 3 — Rename handler           | ⬜ Todo |       |

**US-07b** (ambiguous stem diagnostics) was implemented as part of v0.1 and
requires no work in v0.2.

---

## Step 1 — Config

Read `extensions` and `attachmentsDir` from `initializationOptions`. This is
purely a configuration parsing change — no index or handler behaviour changes yet.

**Deliverables:**

- `Config` gains `extensions: Vec<String>` (default `["md"]`) and
  `attachments_dir: Option<PathBuf>` (default `None`), both parsed from
  `initializationOptions`
- `register_file_watcher` uses `config.extensions` instead of the hardcoded
  `"md"` — one watcher glob registered per extension
- If `config.attachments_dir` is set, a second `RelativePattern` watcher for
  `"**/*"` is registered scoped to that directory
- `index::build` receives the full `Config` (or at minimum `extensions`) rather
  than a bare `&[&str]`

**Unit tests** (`src/server/tests.rs` or inline):

| Test                                  | What it verifies                                                       |
| ------------------------------------- | ---------------------------------------------------------------------- |
| `config_extensions_default`           | No `initializationOptions` → `extensions` is `["md"]`                  |
| `config_extensions_from_options`      | `{"extensions": ["md", "mdx"]}` → `extensions` is `["md", "mdx"]`      |
| `config_attachments_dir_default`      | No `initializationOptions` → `attachments_dir` is `None`               |
| `config_attachments_dir_from_options` | `{"attachmentsDir": "assets"}` → `attachments_dir` is `Some("assets")` |

> **Manual checkpoint:** no visible behaviour change for a default workspace.
> With `KNAP_LOG=debug`, confirm that the watcher registration log line reflects
> the configured extension(s) rather than the hardcoded `"md"`.

---

## Step 2 — Attachment file tracking

Add `by_filename` to `NoteIndex`, extend the startup crawl to register all
non-note files, update `resolve` to fall through to `by_filename`, and wire in
the optional attachment directory watcher. Update diagnostic messages to be
link-agnostic.

**Deliverables:**

- `NoteIndex` gains `by_filename: HashMap<String, Vec<PathBuf>>`
- `Note` gains a `filename()` helper returning the full filename with extension
- `index(note)` also inserts into `by_filename`
- `remove(path)` also removes from `by_filename`
- `index::build()` crawl walks all files; non-note files are registered in
  `by_filename` only (no parsing)
- `resolve(target)` falls through to `by_filename` when `by_stem` finds nothing
- `on_did_change_watched_files` handles non-note-extension events from the
  attachments watcher: add to `by_filename` on `Created`, remove on `Deleted`,
  no-op on `Changed`
- Diagnostic messages updated: `Broken` → `"Link target not found: '[[…]]'"`;
  `Ambiguous` → `"'[[…]]' matches multiple files: …"`

**Unit tests** (`src/index/tests.rs`):

| Test                                 | What it verifies                                                       |
| ------------------------------------ | ---------------------------------------------------------------------- |
| `by_filename_populated_for_note`     | `index(note)` adds the note's full filename to `by_filename`           |
| `by_filename_cleared_on_remove`      | `remove(path)` removes the entry from `by_filename`                    |
| `resolve_falls_through_to_filename`  | Target not in `by_stem` but in `by_filename` → `Found`                 |
| `resolve_prefers_stem_over_filename` | Target in both `by_stem` and `by_filename` → `by_stem` result wins     |
| `resolve_broken_in_both_maps`        | Target in neither map → `Broken`                                       |
| `non_note_file_registered`           | Non-note path registered via `add_attachment` appears in `by_filename` |

**Integration tests** (`tests/diagnostics.rs`, extending existing file):

| Test                                    | What it verifies                                                     |
| --------------------------------------- | -------------------------------------------------------------------- |
| `attachment_link_present_no_diagnostic` | `[[image.png]]` with `image.png` in the index → no diagnostic        |
| `attachment_link_absent_diagnostic`     | `[[image.png]]` with no matching file → `Link target not found`      |
| `diagnostic_message_broken`             | Broken note link now produces `Link target not found: '[[…]]'`       |
| `diagnostic_message_ambiguous`          | Ambiguous note link now produces `'[[…]]' matches multiple files: …` |

> **Manual checkpoint:** place `[[diagram.png]]` in a note. Without the file
> present, a `Link target not found` warning should appear. Copy any `.png` into
> the workspace root (or configured `attachmentsDir`) and restart the server —
> the warning should be gone.

---

## Step 3 — Rename handler

Implement `workspace/willRenameFiles` and advertise the capability. After this
step, renaming a file in the editor updates all `[[links]]` pointing to it.

**Deliverables:**

- `capabilities.rename_provider` advertised in `initialize` response
- `workspace/willRenameFiles` registered via `client/registerCapability` at
  `initialized`
- `handlers::handle_will_rename_files()` — returns a `WorkspaceEdit` with one
  `TextEdit` per backlink, replacing `inner_range` with the new stem
- `dispatch_request` routes `workspace/willRenameFiles` to the handler

**Unit tests** (`src/handlers/tests.rs` or `src/handlers.rs`):

| Test                                 | What it verifies                                                       |
| ------------------------------------ | ---------------------------------------------------------------------- |
| `rename_produces_edits`              | File with two backlinks → `WorkspaceEdit` with two `TextEdit`s         |
| `rename_no_backlinks_empty_edit`     | File with no backlinks → empty `WorkspaceEdit`                         |
| `rename_preserves_alias`             | `[[old\|alias]]` → edit replaces only stem; result is `[[new\|alias]]` |
| `rename_multiple_files_in_one_batch` | Two files renamed together → edits produced for both                   |

**Integration tests** (`tests/rename.rs`):

| Test                                 | What it verifies                                                                                                 |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `will_rename_returns_workspace_edit` | Full round-trip: send `willRenameFiles`, receive `WorkspaceEdit` with correct edits                              |
| `index_consistent_after_rename`      | After applying the edit via `didChange` and the rename via `didChangeWatchedFiles`, `resolve` finds the new stem |

> **Manual checkpoint:** in a workspace with two or three linked notes, rename a
> file using the editor's file-tree rename. All `[[links]]` pointing to the
> renamed file should be updated instantly, with no broken-link diagnostics
> appearing.

---

## Done — v0.2 complete

At this point all five v0.2 user stories are implemented and tested:

| Story                                        | Delivered in step |
| -------------------------------------------- | ----------------- |
| US-04 Rename file → update all `[[links]]`   | Step 3            |
| US-05 Aliased links — rename preserves alias | Step 3            |
| US-07b Ambiguous stem diagnostics            | _(v0.1)_          |
| US-21 Config: file extensions                | Step 1            |
| US-26 Attachment link resolution             | Step 2            |

Final check before tagging: run the full test suite (`cargo test`), run
`cargo clippy -- -D warnings`, then do a manual end-to-end session — open the
workspace, verify completion, definition, and references still work, add an
attachment link, rename a file, confirm all diagnostics and navigation stay
consistent throughout.
