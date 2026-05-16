# v0.3.1 Implementation Plan

Describes the order of changes, what is tested after each step, and manual
checkpoints. Each step produces something testable before the next begins.

All implementation steps follow TDD:

1. Write the tests first (stub signatures to compile if needed)
2. Confirm new tests **fail** before writing implementation
3. Implement until tests pass, then `cargo clippy -- -D warnings` clean

---

## Status

| Step | Description                            | Status |
| ---- | -------------------------------------- | ------ |
| 1    | Internal cleanup (dedup + test helper) |        |
| 2    | Correctness and safety fixes           |        |
| 3    | Documentation drift                    |        |
| 4    | US-46 — Segment-by-segment completion  |        |
| 5    | Integration tests + regression         |        |

---

## Step 1 — Internal Cleanup

Three mechanical changes with no behavioral impact. No new tests needed beyond
confirming `cargo test` stays clean.

### BF-03: Extract `strip_surrounding_quotes`

In `src/parser/mod.rs`, add a private helper before `extract_frontmatter`:

```rust
fn strip_surrounding_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
```

Replace the inline quote-stripping blocks at lines ~150 and ~204 with calls to
`strip_surrounding_quotes(value)` and `strip_surrounding_quotes(raw_value)`.

Verify: `cargo test` passes with no test changes — no behavior changed.

---

### BF-04: Centralise skip-list in `index/mod.rs`

Change `should_skip_dir` in `src/index/mod.rs` from `fn` to `pub(crate) fn`.

In `src/server/mod.rs`, replace the inline `should_skip_path` body with a
delegation to `index::should_skip_dir`:

```rust
fn should_skip_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let std::path::Component::Normal(name) = c else { return false };
        crate::index::should_skip_dir(&name.to_string_lossy())
    })
}
```

Verify: `cargo test` passes, `cargo clippy -- -D warnings` clean.

---

### BF-08: Shared test helper

Create `src/test_helpers.rs`:

```rust
#[cfg(test)]
pub(crate) fn note(path: &str, content: &str) -> crate::parser::Note {
    crate::parser::parse(std::path::Path::new(path), content)
}
```

Declare it in `src/main.rs` (or `src/lib.rs`) as `#[cfg(test)] mod test_helpers;`.

Replace the `note()` helpers inside `#[cfg(test)]` blocks in `src/handlers.rs`
and `src/index/tests.rs` with `use crate::test_helpers::note;`.

Verify: `cargo test` passes with the same test names and outcomes.

---

## Step 2 — Correctness and Safety Fixes

Each sub-item is independent; apply in any order within this step.

### BF-01: EOF frontmatter test

In `src/parser/mod.rs`, add to the existing `tests` module:

```rust
#[test]
fn frontmatter_body_offset_eof_terminated() {
    // No trailing newline after closing ---
    let note = parse(Path::new("x.md"), "---\ntitle: T\n---");
    assert_eq!(note.headings.len(), 0);
    assert_eq!(note.md_links.len(), 0);
    let fm = note.frontmatter.expect("frontmatter present");
    assert_eq!(fm.title.as_deref(), Some("T"));
}
```

No implementation change. Confirm the test passes (it should — this is a
coverage gap, not a bug).

---

### BF-02: Bounded `recv_response`

In `src/cli.rs`, update `recv_response`:

```rust
fn recv_response(conn: &lsp_server::Connection) -> anyhow::Result<lsp_server::Response> {
    let mut skipped = 0u32;
    loop {
        match conn.receiver.recv()? {
            lsp_server::Message::Response(r) => return Ok(r),
            lsp_server::Message::Request(_) | lsp_server::Message::Notification(_) => {
                skipped += 1;
                if skipped > 32 {
                    anyhow::bail!("recv_response: server sent >32 non-response messages");
                }
            }
        }
    }
}
```

No unit test (exercising this requires a fake connection). Verify `cargo test`
and `cargo clippy` are clean.

---

### BF-05: `path.parent()` consistency

In `src/handlers.rs` line ~196, change:

```rust
let from_dir = path.parent().unwrap_or(Path::new(""));
```

to:

```rust
let from_dir = path.parent().expect("indexed path must have a parent");
```

Verify: `cargo test` passes.

---

### BF-06: `path_to_uri` panic message

In `src/handlers.rs` `path_to_uri`, change:

```rust
.expect("non-absolute path")
```

to:

```rust
.unwrap_or_else(|_| panic!("path_to_uri: path must be absolute, got: {}", path.display()))
```

Verify: `cargo test` passes.

---

### BF-07: Warn on extra content changes

In `src/server/mod.rs` `on_did_change`, replace the `into_iter().next()` block:

```rust
let mut iter = params.content_changes.into_iter();
let content = match iter.next() {
    Some(c) => c.text,
    None => {
        warn!("didChange: no content changes");
        return;
    }
};
if iter.next().is_some() {
    warn!("didChange: received >1 content changes; only the first is used (Full sync)");
}
```

Verify: `cargo test` passes.

---

> **Checkpoint after Step 2:** `cargo test` clean, `cargo clippy -- -D warnings`
> clean. Commit: `fix: correctness and safety fixes from code review`.

---

## Step 3 — Documentation Drift

All changes are in Markdown files; no Rust compilation needed.

### BF-09: README

- Line 3 (badge): `version-0.2.0` → `version-0.3.0`
- Line 48 (status): update to `v0.3.0 — Heading Navigation & Anchors`
- "What it does" section: add bullet points for DocumentSymbols, WorkspaceSymbols,
  heading Rename, and Anchor completions

### BF-10: ARCHITECTURE.md handler table

Add four rows to the Request Handlers table (currently stops at v0.2):

| Handler                    | Method                        | Since |
| -------------------------- | ----------------------------- | ----- |
| `handle_document_symbols`  | `textDocument/documentSymbol` | v0.3  |
| `handle_workspace_symbols` | `workspace/symbol`            | v0.3  |
| `handle_prepare_rename`    | `textDocument/prepareRename`  | v0.3  |
| `handle_rename`            | `textDocument/rename`         | v0.3  |

### BF-11: Frontmatter version label

Line ~153: change `_(v0.4)_` → `_(v0.1, extended v0.3)_`

### BF-12: Stale `</thinking>` tag

Delete the `</thinking>` line at the end of `docs/ARCHITECTURE.md`.

---

> **Checkpoint after Step 3:** commit `docs: fix README and ARCHITECTURE drift`.

---

## Step 4 — US-46: Segment-by-segment Completion

This is the most significant step. Work in `src/handlers.rs` exclusively
(no index changes, no server capability change until the end of this step).

### 4a: Add `byte_to_utf16_offset` helper

Add alongside `utf16_to_byte_offset`:

```rust
fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> u32 {
    s[..byte_offset].chars().map(|c| c.len_utf16() as u32).sum()
}
```

No tests needed — this is the inverse of the existing `utf16_to_byte_offset`.

### 4b: Replace `check_link_trigger` with `check_dir_trigger`

Remove `check_link_trigger`. Add:

```rust
/// Returns `Some(partial)` when the cursor is inside a Markdown link destination
/// after `](`, and the destination is not in an anchor context (no `#`).
/// `partial` is the text between `](` and the cursor (empty when cursor is
/// immediately after `](`).
fn check_dir_trigger(content: &str, pos: Position) -> Option<String> {
    let line = content.lines().nth(pos.line as usize).unwrap_or("");
    let cursor = utf16_to_byte_offset(line, pos.character);
    let before = &line[..cursor];
    let open = before.rfind("](")?;
    let after_open = &before[open + 2..];
    if after_open.contains('#') {
        return None;
    }
    Some(after_open.to_string())
}
```

**Write unit tests first** (they will fail until 4c is complete):

| Test                                       | Setup                                    | Assert            |
| ------------------------------------------ | ---------------------------------------- | ----------------- |
| `check_dir_trigger_empty_after_open`       | content `"[x](`, cursor right after `(`  | `Some("")`        |
| `check_dir_trigger_partial_path`           | content `"[x](subdir/`, cursor after `/` | `Some("subdir/")` |
| `check_dir_trigger_none_outside_link`      | `/` typed on bare text line              | `None`            |
| `check_dir_trigger_none_in_anchor_context` | `"[x](path#`                             | `None`            |

### 4c: Implement directory-aware `handle_completion`

Replace the existing file-path completion branch (the `check_link_trigger` block)
with a new `check_dir_trigger` branch. Keep the anchor branch unchanged.

The new branch:

1. Compute `base_dir`:

   ```rust
   let note_dir = path.parent().expect("indexed path must have a parent");
   let base_dir = if partial.ends_with('/') || partial.is_empty() {
       index::normalize_path(&note_dir.join(&*partial))
   } else {
       let p = std::path::Path::new(&partial);
       index::normalize_path(&note_dir.join(p.parent().unwrap_or(std::path::Path::new(""))))
   };
   ```

2. Enumerate immediate children (dirs and files) as described in the design doc.

3. Compute `replace_range`:

   ```rust
   let line = note.content.lines().nth(pos.line as usize).unwrap_or("");
   let cursor_byte = utf16_to_byte_offset(line, pos.character);
   let open_byte = line[..cursor_byte].rfind("](").expect("check_dir_trigger guarantees ](");
   let start_char = byte_to_utf16_offset(line, open_byte + 2);
   let replace_range = Range {
       start: Position { line: pos.line, character: start_char },
       end: pos,
   };
   ```

4. Build `CompletionItem` entries for directories and files, each with a
   `text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: replace_range, new_text }))`.

**Write unit tests first** (fail before implementation):

| Test                                        | What it verifies                                                            |
| ------------------------------------------- | --------------------------------------------------------------------------- |
| `dir_completion_initial_shows_siblings`     | `](` → sibling .md files appear as FILE items                               |
| `dir_completion_initial_shows_subdir`       | `](` → subdirectory appears as FOLDER item with label `subdir/`             |
| `dir_completion_initial_excludes_current`   | Current note not in the list                                                |
| `dir_completion_parent_dir_option`          | Files above → `../` FOLDER item appears                                     |
| `dir_completion_subdir_shows_children`      | `](subdir/` → only files inside `subdir/` appear                            |
| `dir_completion_text_edit_replaces_partial` | `text_edit.range` starts right after `](`; `new_text` is full relative path |
| `dir_completion_title_as_label`             | Note with `title: My Note` → label is `"My Note"`, filter_text is filename  |
| `dir_completion_attachment_filename_label`  | Attachment shows filename as label                                          |

### 4d: Update capability advertisement

In `src/server/mod.rs`, add `/` to `trigger_characters`:

```rust
completion_provider: Some(CompletionOptions {
    trigger_characters: Some(vec!["(".to_string(), "#".to_string(), "/".to_string()]),
    ..Default::default()
}),
```

### 4e: Update existing completion tests

The following tests assert flat-list behavior that the new implementation
no longer provides. Update each to match the segment-by-segment contract:

- `completion_trigger_returns_notes` — if all notes are in the same directory,
  they all still appear (no change needed in a same-dir setup); if the test
  mixes directories, update to expect only same-dir files and `../` entries.
- `completion_relative_path_subdirectory` — change assertion from `subdir/note.md`
  appearing directly to `subdir/` (a FOLDER item) appearing instead.
- `completion_includes_attachments` — if the attachment is in a subdirectory,
  expect a FOLDER item for that dir; if it's in the same dir, expect the file.
- `completion_attachment_label_is_filename` — unchanged if attachment is a sibling.
- `completion_title_used_as_label` — unchanged if the target note is a sibling.

Run `cargo test` and confirm all completion tests pass before proceeding.

---

> **Checkpoint after Step 4:** manually verify in an editor.
>
> Open a vault with notes at multiple directory levels. In a note, type `[link](`.
> Confirm: only siblings and `subdir/` items appear — not every file in the vault.
> Select a directory item. Confirm the editor re-triggers completion (the `/`
> lands and a new picker appears with that directory's contents). Navigate to a
> file two levels deep and confirm the final inserted path is correct. Confirm
> anchor completions (`](file.md#`) are unaffected.

---

## Step 5 — Integration Tests + Regression

Final step: wire the new feature and all fixes into the end-to-end test suite.

### New integration tests (`tests/lsp.rs`)

| Test                                  | What it verifies                                                                |
| ------------------------------------- | ------------------------------------------------------------------------------- |
| `test_dir_completion_initial`         | `textDocument/completion` after `](` returns FOLDER + FILE items, not flat list |
| `test_dir_completion_retrigger_slash` | Completion triggered with `/` context `](subdir/` returns children of `subdir`  |
| `test_anchor_completion_unchanged`    | `](file.md#` still returns heading items with slug inserts                      |

### Regression

Run the full suite:

```
cargo test
cargo clippy -- -D warnings
```

All v0.1, v0.2, and v0.3 integration tests must remain green.

---

> **Final checkpoint:** `cargo test` clean, `cargo clippy -- -D warnings` clean.
> Commit: `feat(US-46): segment-by-segment directory completion`.

---

## Done — v0.3.1 complete

| Item     | Description                               | Delivered in step |
| -------- | ----------------------------------------- | ----------------- |
| BF-01    | EOF frontmatter test                      | Step 2            |
| BF-02    | Bounded `recv_response`                   | Step 2            |
| BF-03    | `strip_surrounding_quotes` extracted      | Step 1            |
| BF-04    | Skip-list centralised in `index`          | Step 1            |
| BF-05    | `path.parent().expect()`                  | Step 2            |
| BF-06    | `path_to_uri` panic message includes path | Step 2            |
| BF-07    | Warn on extra content changes             | Step 2            |
| BF-08    | Shared `test_helpers` module              | Step 1            |
| BF-09–12 | README and ARCHITECTURE.md updated        | Step 3            |
| US-46    | Segment-by-segment directory completion   | Steps 4 + 5       |
