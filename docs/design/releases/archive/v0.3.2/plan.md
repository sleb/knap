# v0.3.2 Implementation Plan

One story, one focused change in `src/handlers.rs`. No index changes, no new
LSP capabilities, no parser changes.

---

## Status

| Step | Description                       | Status  |
| ---- | --------------------------------- | ------- |
| 1    | User story, roadmap, design, plan | ✅ Done |
| 2    | US-47 — Global file items         | ✅ Done |

---

## Step 1 — User story, roadmap, design, plan

Add US-47 to `docs/USER_STORIES.md`, add the v0.3.2 milestone to
`docs/ROADMAP.md`, and create `docs/design/releases/v0.3.2/design.md` and this
plan file.

---

## Step 2 — US-47: Global file items in directory completion

### Change

In `handle_completion` in `src/handlers.rs`, after emitting tier-0 FOLDER items
and tier-1 immediate FILE items, emit a tier-2 item for every workspace file not
already shown as an immediate child and not the current file.

**Add `sort_text` to existing items** so tiers order correctly:

- FOLDER items: `sort_text = Some(format!("0_{dir_name}"))`
- Immediate FILE items: `sort_text = Some(format!("1_{file_name}"))`

**Track immediate files** to avoid duplicates:

```rust
let immediate_set: HashSet<&PathBuf> = files.iter().collect();
```

**Emit global items** for every note and attachment not in `immediate_set`:

```rust
for file_path in note_paths.iter().chain(attach_paths.iter()) {
    if file_path.as_path() == path.as_path() { continue; }
    if immediate_set.contains(file_path) { continue; }
    let full_rel = relative_path(note_dir, file_path);
    let file_name = file_path.file_name()…;
    let label = index.get_note(file_path)
        .and_then(|n| n.frontmatter.as_ref()?.title.clone())
        .unwrap_or_else(|| file_name.clone());
    items.push(CompletionItem {
        label,
        kind: Some(CompletionItemKind::FILE),
        filter_text: Some(full_rel.clone()),   // full path for deep matching
        sort_text:   Some(format!("2_{full_rel}")),
        detail:      Some(full_rel.clone()),   // shown as secondary text
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace_range,              // same range: replaces ](…cursor
            new_text: full_rel,
        })),
        ..Default::default()
    });
}
```

### Test updates

One existing unit test (`completion_relative_path_subdirectory`) and one
integration test (`test_dir_completion_initial`) previously asserted that deep
files did **not** appear directly. Both assertions are inverted: the tests now
assert that `sub/b.md` **does** appear as a global item alongside the `sub/`
FOLDER item.

### Regression

```
cargo test
cargo clippy -- -D warnings
```

All 162 tests must pass (136 unit + 26 integration).

---

> **Final commit:** `feat: show global file list alongside directory items in completion`
