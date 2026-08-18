# v0.17 Implementation Plan — Directory Links

Describes the order in which changes are made, what is tested after each
step, and the checkpoints where the server should be manually verified
against a real editor.

The guiding principle: each step produces something testable. No step lays
down untested code for the next step to build on.

---

## Status

| Step                                                     | Status | Notes |
| -------------------------------------------------------- | ------ | ----- |
| 1 — `NoteIndex`: `all_dirs` and directory primitives     | Done   |       |
| 2 — Initial crawl registers directories                  | Done   |       |
| 3 — Live discovery: `register_ancestor_dirs`             | Done   |       |
| 4 — Definition, References, Diagnostics, Code Actions    | Done   |       |
| 5 — Completion: index-backed dirs + "accept this folder" | Done   |       |
| 6 — Integration tests                                    | Todo   |       |

---

## Step 1 — `NoteIndex`: `all_dirs` and directory primitives

Data model first: add the field and the four methods that read/write it,
independent of anything that populates it yet (tests seed `all_dirs`
directly). This is TDD — the tests below are the spec for `resolve()`,
`add_dir`, `remove_dir`, `is_dir_indexed`, and `child_dirs` before any of
them exist.

1. Write all unit tests for this step first — add `all_dirs: HashSet<PathBuf>`
   to the struct definition and stub `add_dir`/`remove_dir`/`is_dir_indexed`/
   `child_dirs`/`target_exists` with `todo!()` bodies (and a `#[cfg(test)]`
   seeding helper, e.g. `pub(crate) fn seed_dir(&mut self, path: PathBuf)`,
   alongside the existing `seed()`) so the crate compiles.
2. Run `cargo test` and confirm the new tests **fail** (panic on `todo!()`).
3. Implement `target_exists`, wire it into `resolve()`, `index()` step 3, and
   `recheck_incoming()`; implement `add_dir`/`remove_dir`/`is_dir_indexed`/
   `child_dirs` for real. Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `all_dirs: HashSet<PathBuf>` field on `NoteIndex` (`src/index/mod.rs`)
- `fn target_exists(&self, path: &Path) -> bool` — private, used by
  `resolve()`/`index()`/`recheck_incoming()` in place of their direct
  `all_files.contains(...)` checks
- `pub fn add_dir(&mut self, path: PathBuf) -> IndexDelta`
- `pub fn remove_dir(&mut self, path: &Path) -> IndexDelta`
- `pub fn is_dir_indexed(&self, path: &Path) -> bool`
- `pub fn child_dirs(&self, dir: &Path) -> impl Iterator<Item = &Path>`
- `#[cfg(test)] pub(crate) fn seed_dir(&mut self, path: PathBuf)` test helper

**Unit tests:**

| Test                                         | What it verifies                                                                                             |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `resolve_existing_dir_found`                 | `resolve()` returns `Found` for a target that normalizes to a directory registered via `seed_dir`            |
| `resolve_nonexistent_dir_broken`             | `resolve()` returns `Broken` for a directory-shaped target that was never registered                         |
| `index_populates_links_to_for_dir_target`    | indexing a note whose link targets a `seed_dir`-registered directory populates `links_to` for it             |
| `add_dir_resolves_previously_broken_link`    | `add_dir` on a path a note already links to flips that note into the returned `affected_paths`               |
| `remove_dir_breaks_links`                    | `remove_dir` on a linked-to directory returns the linking note's path in `affected_paths`                    |
| `is_dir_indexed_true_for_known_dir`          | `is_dir_indexed` is `true` only for a directory registered via `add_dir`/`seed_dir`, `false` otherwise       |
| `child_dirs_returns_immediate_children_only` | `child_dirs` returns directories whose parent is exactly the queried directory, excluding deeper descendants |
| `child_dirs_includes_empty_directory`        | an empty directory registered via `add_dir` appears in its parent's `child_dirs`                             |

> **Manual checkpoint:** No editor checkpoint — `NoteIndex` isn't wired into
> the crawl or the server yet. Covered entirely by unit tests.

---

## Step 2 — Initial crawl registers directories

Wire `all_dirs` into `index::build()` so a real vault gets its directories
registered on startup, before any live-update path exists. This is the
first step with an editor-visible effect — a `.md` file with a directory
link stops being flagged broken on `knap lsp` startup.

**Deliverables:**

- `walk_dir` in `src/index/mod.rs` gains an `out_dirs: &mut Vec<PathBuf>`
  parameter; pushes each non-skipped directory's normalized path before
  recursing into it
- `walk_files` renamed `walk_files_and_dirs`, returns `(Vec<PathBuf>,
Vec<PathBuf>)`
- `build()` registers each root (via `normalize_path`) and every walked
  directory via `index.add_dir(..)` before indexing any file from that root

**Unit tests:**

| Test                                              | What it verifies                                                                                                                   |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `build_registers_every_directory_including_empty` | `build()` over a fixture tree with an empty subdirectory registers it as a known directory (`is_dir_indexed`)                      |
| `build_registers_workspace_root_as_dir`           | `build()` registers each root itself so a link to `..` from a first-level note resolves                                            |
| `build_excludes_dir_matching_exclude_pattern`     | a directory matching a `knap.toml` `exclude` glob is never registered (mirrors `build_excluded_file_not_registered_as_attachment`) |

> **Manual checkpoint:** In a scratch vault, create `docs/lld/` containing
> one file, and a note with `[LLDs](docs/lld/)`. Run `knap lint .` — no
> `broken-link` diagnostic for that line. Also create an empty `docs/empty/`
> and a link to it; confirm it's clean too.

---

## Step 3 — Live discovery: `register_ancestor_dirs`

Directories created after startup become visible without a restart, bounded
to the configured index roots so climbing can't run away.

1. Write the unit tests below first, with `register_ancestor_dirs` stubbed
   to `unimplemented!()`.
2. Run `cargo test` and confirm they **fail**.
3. Implement `register_ancestor_dirs` in `src/server/mod.rs`; call it after
   `index.index(note)` in `on_did_open`/`on_did_change`'s note branch and
   after both `index.index(note)`/`index.add_attachment(path)` in
   `on_did_change_watched_files`, extending each call site's existing
   `affected_paths` before calling `publish_diagnostics`. Run
   `cargo clippy -- -D warnings`.

**Deliverables:**

- `fn register_ancestor_dirs(path: &Path, roots: &[PathBuf], index: &mut NoteIndex) -> HashSet<PathBuf>` in `src/server/mod.rs`
- Call sites updated in `on_did_open`, `on_did_change`, and
  `on_did_change_watched_files` (both the note and attachment branches of
  the `Created`/`Changed` arms)

**Unit tests:**

| Test                                                 | What it verifies                                                                                                                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `register_ancestor_dirs_stops_at_known_ancestor`     | climbing halts at the first already-known directory; `add_dir` isn't called for anything above it (assert via `affected_paths` staying empty when every ancestor is pre-seeded) |
| `register_ancestor_dirs_registers_new_nested_dirs`   | a file two levels under a previously-unknown directory registers both new ancestor directories in `affected_paths`                                                              |
| `register_ancestor_dirs_no_root_match_returns_empty` | a path outside every configured root returns an empty set without panicking                                                                                                     |

> **Manual checkpoint:** With `knap lsp` already running against the scratch
> vault from Step 2, create a brand-new `docs/new-section/` folder with a
> file in it from outside the editor (e.g. a terminal `mkdir` + `echo > `),
> and in an already-open note add `[New](docs/new-section/)`. Save. The
> broken-link diagnostic on that line should clear without restarting the
> server.

---

## Step 4 — Definition, References, Diagnostics, Code Actions

These four handlers need no code changes — they're pure consumers of
`resolve()`/`links_to()`/`get_note()`, all already directory-aware after
Steps 1–3. This step is entirely regression coverage proving that claim,
plus the one genuinely new behavior (a directory-with-anchor link) each
handler needs to get right by falling through its existing "no note at this
path" branch.

1. Write the unit tests below first.
2. Run `cargo test` — most should already **pass** (confirming the "no code
   change needed" claim); any that fail point at a spot where a handler
   _does_ touch `all_files`/`by_path` directly instead of going through
   `resolve()`/`get_note()`, which must be fixed before continuing.
3. `cargo clippy -- -D warnings`.

**Deliverables:**

- No production code changes expected in `src/handlers.rs`. If a test in
  this step fails, the deliverable becomes fixing the one call site that
  bypassed `resolve()`, `links_to()`, or `get_note()`.

**Unit tests:**

| Test                                                              | What it verifies                                                                                               |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `handle_definition_directory_link_returns_location`               | Go to Definition on a link to an existing directory returns a `Location` at that directory, `Range::default()` |
| `handle_definition_directory_link_with_anchor_returns_no_heading` | a directory link with an anchor falls back to `Range::default()` (no headings to match)                        |
| `handle_references_directory_link_returns_backlinks`              | Find References from a link to a directory returns every other note linking to that same directory             |
| `compute_diagnostics_no_broken_link_for_existing_dir`             | a link to an existing directory produces no `broken-link` diagnostic                                           |
| `compute_diagnostics_broken_anchor_on_dir_link`                   | `[x](docs/#foo)` produces a `broken-anchor` diagnostic (directories have no headings)                          |
| `handle_code_actions_no_anchor_fix_offered_for_dir_link`          | a broken-anchor directory link offers zero "Change anchor to..." actions (mirrors the attachment case)         |

> **Manual checkpoint:** In the scratch vault, put the cursor on
> `[LLDs](docs/lld/)` and trigger Go to Definition — the editor navigates to
> (or reveals) the `docs/lld/` directory. From a second note also linking to
> `docs/lld/`, trigger Find References on the first note's link — both notes
> appear in the results.

---

## Step 5 — Completion: index-backed dirs + "accept this folder"

Directory-trigger completion switches from inferring directories out of
file paths to reading `index.child_dirs`, and gains the new terminal item.

1. Write the unit tests below first, against the current (file-inferred)
   implementation where they'd fail — `completion_dir_trigger_lists_child_dirs_including_empty`
   fails because an empty directory never appears today; the "accept item"
   tests fail because no such item exists yet.
2. Run `cargo test` and confirm they **fail**.
3. Replace the `dirs` computation with `index.child_dirs(&base_dir)` and add
   the new item per the design. Run `cargo clippy -- -D warnings`.

**Deliverables:**

- `handle_completion`'s directory-trigger branch (`src/handlers.rs`):
  `dirs` sourced from `index.child_dirs(&base_dir)` instead of file-path
  inference; new "accept this folder" `CompletionItem` pushed when
  `base_dir != note_dir && index.is_dir_indexed(&base_dir)`

**Unit tests:**

| Test                                                          | What it verifies                                                                                                           |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `completion_dir_trigger_lists_child_dirs_including_empty`     | directory-trigger completion includes a subdirectory with no files in it                                                   |
| `completion_dir_trigger_offers_accept_item_when_drilled_in`   | completing inside `docs/lld/` includes a FOLDER item labeled `"lld"` (no trailing slash) whose `new_text` is `"docs/lld/"` |
| `completion_dir_trigger_no_accept_item_at_note_own_dir`       | completing at the note's own directory (nothing typed yet) offers no "accept this folder" item                             |
| `completion_dir_trigger_no_accept_item_for_unindexed_partial` | a typed-but-nonexistent directory prefix offers no "accept this folder" item                                               |

> **Manual checkpoint:** In the scratch vault, in a note, type `[LLDs](docs/`
> and select `lld/` from the completion list (re-triggers into `lld/`'s
> contents). The list now also shows a `lld` item (no trailing slash,
> detail "Link to this folder"), sorted first. Select it — the link becomes
> `[LLDs](docs/lld/)` with the cursor past `)`, no further completion popup.

---

## Step 6 — Integration tests

End-to-end coverage over the full LSP message loop, including the live
directory-creation path from Step 3. Always last.

**Deliverables:**

- New tests added to `tests/lsp.rs`
- `cargo test` passes, `cargo clippy -- -D warnings` clean

| Test                                              | What it verifies                                                                                                                                                                              |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `directory_link_resolves_end_to_end`              | a workspace with a note linking to an existing subdirectory reports no diagnostics after `initialize`                                                                                         |
| `directory_link_definition_and_references`        | `textDocument/definition` on the directory link returns the directory's `Location`; `textDocument/references` from another note linking to the same directory returns both                    |
| `directory_created_live_resolves_without_restart` | a `didChangeWatchedFiles` `Created` event for a new note under a brand-new nested directory clears a previously-broken directory-link diagnostic in another open note, with no server restart |

> **Manual checkpoint (full session):** Open the editor on a real vault with
> nested folders. Link to a folder, confirm no diagnostic, Go to Definition,
> Find References, and completion's "accept this folder" item all work as
> in Steps 2–5. Create a new nested folder with a file in it live, link to
> it from an open note, confirm the diagnostic clears without restarting.
> Confirm earlier releases (file links, anchors, tags, rename, backlinks)
> are unaffected.

---

## Done — v0.17 Directory Links complete

| Story | Feature                                                                                                                         | Delivered in step |
| ----- | ------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| US-56 | Links to an existing directory resolve (no broken-link diagnostic); Go to Definition navigates to it; Find References tracks it | Steps 1–4, 6      |
| US-57 | Path completions let a directory be accepted as the finished link target, not just a step to drill further into                 | Step 5, 6         |
