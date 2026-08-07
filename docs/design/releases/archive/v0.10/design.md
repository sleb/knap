# v0.10 Design — Tag Rename

Covers the stories in the v0.10 release:

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-37 | Rename tag — update all frontmatter occurrences across the workspace atomically |

---

## Goal

A writer can rename a tag — say `rust` → `systems-programming` — and have every
note in the workspace updated in a single editor operation, with no
find-and-replace. The rename dialog pre-fills with the tag text so the writer
never types from scratch. This story ships alone because it extends the existing
rename infrastructure (already in use for headings) without requiring any data
model changes: the parser already records per-tag ranges, and the index already
indexes by tag.

---

## Handler Changes

### `handle_prepare_rename` (`textDocument/prepareRename`)

The handler currently inspects the cursor position and returns a response only
when the cursor is on a heading line. Tag rename extends it to also return a
response when the cursor is on a frontmatter tag. The priority order is: check
for a tag first, then fall through to the heading check.

```rust
pub(crate) fn handle_prepare_rename(
    params: TextDocumentPositionParams,
    index: &NoteIndex,
) -> Option<PrepareRenameResponse>
```

The existing `find_tag_at_position` helper (already shared by definition and
references) provides the tag detection:

```rust
// Tag: prepare rename shows the tag's exact text range.
if let Some(tag) = find_tag_at_position(note, pos) {
    return Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: tag.range,
        placeholder: tag.name.clone(),
    });
}
// Heading: existing logic unchanged.
```

`tag.range` covers only the tag text itself (not the surrounding YAML
punctuation), so the editor's rename dialog pre-fills cleanly and highlights the
right text regardless of whether the tag is in a bare scalar, inline list, or
block list form. The disk-parse fallback for un-indexed files is retained and
covers tag detection the same way it covers headings.

---

### `handle_rename` (`textDocument/rename`)

The handler currently renames a heading and its anchor links. Tag rename extends
it to also rename all occurrences of a tag across the workspace when the cursor
is on a tag.

```rust
pub(crate) fn handle_rename(
    params: RenameParams,
    index: &NoteIndex,
) -> Option<WorkspaceEdit>
```

Priority order: check for a tag first, then fall through to the heading logic.

```rust
// Tag rename: replace every occurrence of this tag name (case-insensitive)
// across the workspace.
if let Some(tag) = find_tag_at_position(note, pos) {
    let old_name = tag.name.clone();
    let mut changes: HashMap<lsp_types::Uri, Vec<TextEdit>> = HashMap::new();

    // Always handle the current note directly — covers both the indexed case
    // and the disk-parse fallback (where the note is absent from the index).
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
            continue; // already handled above
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
// Heading: existing logic unchanged.
```

**Tag matching is case-insensitive** (`eq_ignore_ascii_case`): if a workspace
contains notes with `Rust`, `rust`, and `RUST`, all three occurrences are
included in the workspace edit. The replacement text is whatever the user typed
in the rename dialog — casing is not normalised.

**Three YAML forms** (bare scalar `tags: rust`, inline list `tags: [rust, go]`,
block list `tags:\n  - rust`) are all handled identically because the parser
already records a per-`Tag` LSP range for the tag text in each form. The
`TextEdit` replaces that range with the new name in all three cases.

---

## Testing

### Unit tests — `src/handlers.rs`

| Test                                               | What it verifies                                                           |
| -------------------------------------------------- | -------------------------------------------------------------------------- |
| `prepare_rename_tag_returns_range_and_placeholder` | returns `RangeWithPlaceholder` with tag range and tag name as placeholder  |
| `prepare_rename_tag_bare_scalar`                   | works when tag is a bare scalar (`tags: rust`)                             |
| `prepare_rename_tag_inline_list`                   | works when tag is inside an inline list (`tags: [rust, go]`)               |
| `prepare_rename_tag_block_list`                    | works when tag is a block-list item (`- rust` under `tags:`)               |
| `prepare_rename_heading_unchanged`                 | cursor on a heading still returns heading range (no regression)            |
| `prepare_rename_outside_tag_returns_none`          | cursor outside any tag or heading returns `None`                           |
| `rename_tag_updates_all_notes`                     | workspace edit covers every note carrying the old tag                      |
| `rename_tag_case_insensitive_match`                | notes with `Rust`, `rust`, `RUST` are all included in the edit             |
| `rename_tag_replaces_correct_range`                | `TextEdit.range` matches the `Tag.range` from the parsed note              |
| `rename_tag_only_matching_tag_edited`              | sibling tags in the same file that do not match are not included           |
| `rename_tag_current_note_always_included`          | current note is updated even when not present in the index                 |
| `rename_heading_unchanged`                         | cursor on a heading still renames heading and anchor links (no regression) |

### Integration tests (`tests/lsp.rs`)

| Test                            | What it verifies                                                            |
| ------------------------------- | --------------------------------------------------------------------------- |
| `prepare_rename_tag_round_trip` | full session: `prepareRename` on a tag returns the tag's range and name     |
| `rename_tag_round_trip`         | full session: `rename` on a tag returns a workspace edit covering all notes |
