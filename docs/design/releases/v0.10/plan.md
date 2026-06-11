# v0.10 Implementation Plan

Describes the order in which changes are made, what is tested after each step,
and the checkpoints where the server should be manually verified against a real
editor.

The guiding principle: each step produces something testable. No step lays down
untested code for the next step to build on.

---

## Status

| Step                       | Status | Notes |
| -------------------------- | ------ | ----- |
| 1 — Extend `prepareRename` | Done   |       |
| 2 — Extend `rename`        | Done   |       |
| 3 — Integration tests      | Todo   |       |

---

## Step 1 — Extend `handle_prepare_rename` to detect tags

`handle_prepare_rename` currently returns `None` when the cursor is not on a
heading. This step adds a prior check: if the cursor is on a frontmatter tag,
return a `RangeWithPlaceholder` response for it.

This step uses TDD:

1. Write all unit tests for this step first — stub the tag branch as `None` if
   needed to compile.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- In `src/handlers.rs`, add the tag check at the top of `handle_prepare_rename`
  (before the heading check), using the existing `find_tag_at_position` helper:
  ```rust
  if let Some(tag) = find_tag_at_position(note, pos) {
      return Some(PrepareRenameResponse::RangeWithPlaceholder {
          range: tag.range,
          placeholder: tag.name.clone(),
      });
  }
  ```
- Unit tests in `src/handlers.rs` (inside the existing `#[cfg(test)]` module)
  for all six test cases in the table below.

**Unit tests:**

| Test                                               | What it verifies                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------- |
| `prepare_rename_tag_returns_range_and_placeholder` | returns `RangeWithPlaceholder` with tag range and tag name as placeholder |
| `prepare_rename_tag_bare_scalar`                   | works when tag is a bare scalar (`tags: rust`)                            |
| `prepare_rename_tag_inline_list`                   | works when tag is inside an inline list (`tags: [rust, go]`)              |
| `prepare_rename_tag_block_list`                    | works when tag is a block-list item (`- rust` under `tags:`)              |
| `prepare_rename_heading_unchanged`                 | cursor on a heading still returns heading range (no regression)           |
| `prepare_rename_outside_tag_returns_none`          | cursor outside any tag or heading returns `None`                          |

> **Manual checkpoint:** Open a note with `tags: rust` in the frontmatter. Place
> the cursor on the word `rust` and trigger rename (F2 in VS Code / Zed). The
> rename dialog should appear pre-filled with `rust`. Cancel the dialog without
> applying. Then place the cursor on a heading and confirm the heading rename
> dialog still appears correctly.

---

## Step 2 — Extend `handle_rename` to build a tag workspace edit

`handle_rename` currently renames headings only. This step adds a prior check:
if the cursor is on a tag, collect `TextEdit`s for every occurrence of that tag
name (case-insensitive) across all indexed notes, plus the current note directly,
and return a `WorkspaceEdit`.

This step uses TDD:

1. Write all unit tests for this step first — stub the tag branch as `None` if
   needed to compile.
2. Run `cargo test` and confirm the new tests **fail**.
3. Implement until tests pass, then run `cargo clippy -- -D warnings`.

**Deliverables:**

- In `src/handlers.rs`, add the tag branch at the top of `handle_rename` (before
  the heading logic):

  ```rust
  if let Some(tag) = find_tag_at_position(note, pos) {
      let old_name = tag.name.clone();
      let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

      // Current note first (handles disk-parse fallback where note is not indexed).
      let uri = path_to_uri(&path);
      for t in note.frontmatter.iter().flat_map(|fm| &fm.tags) {
          if t.name.eq_ignore_ascii_case(&old_name) {
              changes.entry(uri.clone()).or_default().push(TextEdit {
                  range: t.range,
                  new_text: params.new_name.clone(),
              });
          }
      }

      // All other indexed notes carrying this tag.
      for other in index.notes_by_tag(&old_name) {
          if other.path == path {
              continue;
          }
          let other_uri = path_to_uri(&other.path);
          for t in other.frontmatter.iter().flat_map(|fm| &fm.tags) {
              if t.name.eq_ignore_ascii_case(&old_name) {
                  changes
                      .entry(other_uri.clone())
                      .or_default()
                      .push(TextEdit { range: t.range, new_text: params.new_name.clone() });
              }
          }
      }

      return Some(WorkspaceEdit { changes: Some(changes), ..Default::default() });
  }
  ```

- Unit tests in `src/handlers.rs` for all six test cases in the table below.

**Unit tests:**

| Test                                      | What it verifies                                                           |
| ----------------------------------------- | -------------------------------------------------------------------------- |
| `rename_tag_updates_all_notes`            | workspace edit covers every note carrying the old tag                      |
| `rename_tag_case_insensitive_match`       | notes with `Rust`, `rust`, `RUST` are all included in the edit             |
| `rename_tag_replaces_correct_range`       | `TextEdit.range` matches the `Tag.range` from the parsed note              |
| `rename_tag_only_matching_tag_edited`     | sibling tags in the same file that do not match are not included           |
| `rename_tag_current_note_always_included` | current note is updated even when not present in the index                 |
| `rename_heading_unchanged`                | cursor on a heading still renames heading and anchor links (no regression) |

> **Manual checkpoint:** Open two notes, each with `tags: rust` in their
> frontmatter. Place the cursor on `rust` in one note and trigger rename (F2).
> Type `systems` and confirm. Both files should have `tags: systems` applied
> atomically. Undo and verify both files revert. Then confirm heading rename on
> a separate note still works.

---

## Step 3 — Integration tests

End-to-end tests over the full LSP message loop. Always the last step.

**Deliverables:**

- Two new test functions in `tests/lsp.rs`:
  - `prepare_rename_tag_round_trip` — initialises the server with a workspace
    containing a note that has a frontmatter tag, sends `textDocument/prepareRename`
    with the cursor on the tag, asserts the response is a `RangeWithPlaceholder`
    whose `placeholder` matches the tag text.
  - `rename_tag_round_trip` — initialises the server with two notes each carrying
    the same tag, sends `textDocument/rename` with the cursor on the tag in one
    note, asserts the response is a `WorkspaceEdit` with `TextEdit`s covering
    both files.
- `cargo test` passes, `cargo clippy -- -D warnings` clean.

| Test                            | What it verifies                                                            |
| ------------------------------- | --------------------------------------------------------------------------- |
| `prepare_rename_tag_round_trip` | full session: `prepareRename` on a tag returns the tag's range and name     |
| `rename_tag_round_trip`         | full session: `rename` on a tag returns a workspace edit covering all notes |

> **Manual checkpoint (full session):** Open a real vault with several notes
> sharing a common tag. Trigger rename on the tag in any note, type a new name,
> and confirm. All files in the vault should show the new tag. Open git diff to
> verify only the `tags:` lines changed. Then confirm heading rename, file
> rename, and completion all work as before.

---

## Done — v0.10 complete

| Story | Feature                                                                         | Delivered in step |
| ----- | ------------------------------------------------------------------------------- | ----------------- |
| US-37 | Rename tag — update all frontmatter occurrences across the workspace atomically | Step 2            |
