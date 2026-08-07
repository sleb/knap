# v0.2 Design — Rename & Refactor

Covers the stories in the v0.2 release:

| Story | Feature                                                                 |
| ----- | ----------------------------------------------------------------------- |
| US-04 | Rename file → all standard Markdown links updated (incoming + outgoing) |
| US-26 | Attachment links resolve cleanly — no false broken-link diagnostics     |
| US-44 | Path completions inside `[text](` include non-Markdown files            |
| US-21 | Config: `extensions` selects which file types are treated as notes      |

---

## Goal

Reorganizing your workspace doesn't break links. A writer can rename a file and
have knap atomically update every standard Markdown link that touches that file:
incoming links in other notes recompute their relative paths to the new location,
and outgoing links inside the renamed file recompute their relative paths from the
new base directory. Both are delivered in a single `WorkspaceEdit` before the
rename is applied, so the editor sees a clean, consistent state.

Non-Markdown files (images, PDFs, attachments) become first-class workspace
members in this release: they appear in path completions alongside notes and their
links resolve without false diagnostics — without any extra configuration.

---

## Config Changes

Remove `attachments_dir` from `Config` and `InitOptions`. All files in the
workspace are now indexed at startup via the existing directory crawl, and the
file watcher is broadened to cover all files — no extra configuration required.

`extensions` remains, defaulting to `["md"]`. This controls which files are
parsed as notes (and thus appear in the note index); everything else is indexed
in `all_files` as an attachment.

```rust
// Removed from Config and InitOptions:
// attachments_dir: Option<PathBuf>,
// attachments_dir: Option<String>,
```

US-21 is delivered by the existing `extensions` field — it was plumbed in v0.1
but not documented as a shipped story. v0.2 adds the attachment-watcher changes
that make the full note-vs-attachment distinction work correctly at runtime, so
US-21 is claimed here.

---

## Index Changes

Add one method to `NoteIndex` to expose attachment paths for the completion
handler. Attachments are files in `all_files` that are not parsed notes (i.e.,
not in `by_path`).

```rust
/// All non-note files registered in the workspace (images, PDFs, etc.).
pub fn all_attachment_paths(&self) -> impl Iterator<Item = &PathBuf> {
    self.all_files.iter().filter(|p| !self.by_path.contains_key(*p))
}
```

No other index changes are needed. The rename handler (`handle_will_rename_files`)
reads existing index data via `links_to` and `get_note` — no new index methods
required.

---

## Handler Changes

### `handle_will_rename_files` (new — `workspace/willRenameFiles`)

Called by the editor before it moves a file. Returns a `WorkspaceEdit` that
updates every standard Markdown link affected by the rename so the editor can
apply them atomically.

```rust
pub fn handle_will_rename_files(
    params: RenameFilesParams,
    index: &NoteIndex,
) -> WorkspaceEdit
```

For each `FileRename { old_uri, new_uri }`:

1. **Incoming links** — other notes that link TO the old path.

   For each `LocatedLink` in `index.links_to(&old_path)`:
   - `new_target = relative_path(source_dir, &new_path)`
   - Emit `TextEdit { range: located.md_link.target_range, new_text: new_target }`
     in `located.source_path`.

2. **Outgoing links** — relative links inside the renamed note, which are now
   computed from the wrong base directory.

   For each `MarkdownLink` in `index.get_note(&old_path).md_links`, skip if
   `target` is empty or a URL. Otherwise:
   - `abs_target = normalize_path(old_dir.join(&link.target))`
   - `new_target = relative_path(new_dir, &abs_target)`
   - If `new_target != link.target`, emit `TextEdit { range: link.target_range,
new_text: new_target }` in `old_path`.

Edits are grouped by file URI in `WorkspaceEdit.changes`. A note can appear in
both incoming and outgoing groups (if it links to the renamed file and the renamed
file links back), but the ranges are disjoint so the edits compose cleanly.

```rust
// Skeleton
pub fn handle_will_rename_files(
    params: RenameFilesParams,
    index: &NoteIndex,
) -> WorkspaceEdit {
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

    for file_rename in params.files {
        let Some(old_path) = uri_to_path(&file_rename.old_uri) else { continue };
        let Some(new_path) = uri_to_path(&file_rename.new_uri) else { continue };
        let old_dir = old_path.parent().unwrap_or(Path::new(""));
        let new_dir = new_path.parent().unwrap_or(Path::new(""));

        // Incoming: other notes linking to old_path
        for located in index.links_to(&old_path) {
            let source_dir = located.source_path.parent().unwrap_or(Path::new(""));
            let new_target = relative_path(source_dir, &new_path);
            changes
                .entry(path_to_uri(&located.source_path))
                .or_default()
                .push(TextEdit { range: located.md_link.target_range, new_text: new_target });
        }

        // Outgoing: links inside the renamed file
        if let Some(note) = index.get_note(&old_path) {
            for link in &note.md_links {
                if link.target.is_empty() || looks_like_url(&link.target) {
                    continue;
                }
                let abs_target = normalize_path(&old_dir.join(&link.target));
                let new_target = relative_path(new_dir, &abs_target);
                if new_target != link.target {
                    changes
                        .entry(path_to_uri(&old_path))
                        .or_default()
                        .push(TextEdit { range: link.target_range, new_text: new_target });
                }
            }
        }
    }

    WorkspaceEdit { changes: Some(changes), ..Default::default() }
}
```

### `handle_completion` (updated — US-44)

After emitting one `CompletionItem` per note, also emit one per attachment path:

```rust
// Append after the notes iterator
.chain(
    index.all_attachment_paths()
        .map(|p| {
            let rel = relative_path(from_dir, p);
            let label = p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| rel.clone());
            CompletionItem {
                label,
                kind: Some(CompletionItemKind::FILE),
                filter_text: Some(rel.clone()),
                insert_text: Some(rel),
                ..Default::default()
            }
        })
)
```

Attachments use the filename as the label (no frontmatter title). `filter_text`
equals the relative path so the editor filters by path as the user types, consistent
with note items.

---

## Protocol Handler Changes

### Capabilities

Advertise `workspace.file_operations.will_rename` so editors know to call
`workspace/willRenameFiles`:

```rust
ServerCapabilities {
    // ... existing v0.1 capabilities ...
    workspace: Some(WorkspaceServerCapabilities {
        file_operations: Some(ServerCapabilitiesFileOperations {
            will_rename: Some(FileOperationRegistrationOptions {
                filters: vec![FileOperationFilter {
                    scheme: Some("file".to_string()),
                    pattern: FileOperationPattern {
                        glob: "**/*".to_string(),
                        ..Default::default()
                    },
                }],
            }),
            ..Default::default()
        }),
        ..Default::default()
    }),
    ..Default::default()
}
```

### File watcher

Replace the per-extension globs and optional `attachments_dir` watcher with one
broad watcher per workspace root:

```rust
let watchers: Vec<FileSystemWatcher> = config
    .index_roots
    .iter()
    .filter_map(|root| {
        let base_url = url::Url::from_file_path(root).ok()?;
        let base_uri: Uri = base_url.as_str().parse().ok()?;
        Some(FileSystemWatcher {
            glob_pattern: GlobPattern::Relative(RelativePattern {
                base_uri: OneOf::Right(base_uri),
                pattern: "**/*".to_string(),
            }),
            kind: None,
        })
    })
    .collect();
```

Add a `should_skip_path` guard at the top of `on_did_change_watched_files` event
processing, reusing the same logic as `should_skip_dir` in the index crawler:

```rust
fn should_skip_path(path: &Path) -> bool {
    path.components().any(|c| {
        let Component::Normal(name) = c else { return false };
        let s = name.to_string_lossy();
        s.starts_with('.') || matches!(s.as_ref(), "node_modules" | "target")
    })
}
```

### Dispatch

Add `workspace/willRenameFiles` to `dispatch_request`:

```rust
"workspace/willRenameFiles" => {
    let edit = serde_json::from_value::<RenameFilesParams>(req.params)
        .ok()
        .map(|params| handlers::handle_will_rename_files(params, index))
        .unwrap_or_default();
    connection.sender.send(Message::Response(Response::new_ok(req.id, edit)))?;
}
```

---

## Testing

### Unit tests

| File                | Test                                        | What it verifies                                                        |
| ------------------- | ------------------------------------------- | ----------------------------------------------------------------------- |
| `handlers/tests.rs` | `rename_updates_incoming_links`             | Incoming link `target_range` rewritten with new relative path           |
| `handlers/tests.rs` | `rename_updates_outgoing_links`             | Links inside the renamed note rewritten from new base dir               |
| `handlers/tests.rs` | `rename_updates_both_incoming_and_outgoing` | A note that links to and is linked from `old_path` gets both edits      |
| `handlers/tests.rs` | `rename_skips_url_targets`                  | External URL links are not touched                                      |
| `handlers/tests.rs` | `rename_no_changes_same_dir`                | Rename within same directory leaves sibling-relative links unchanged    |
| `handlers/tests.rs` | `completion_includes_attachments`           | Attachment paths appear in completion items alongside notes             |
| `handlers/tests.rs` | `completion_attachment_label_is_filename`   | Attachment item label is the filename, insert_text is the relative path |
| `index/tests.rs`    | `all_attachment_paths_excludes_notes`       | Only non-note files returned by `all_attachment_paths`                  |

### Integration tests (`tests/lsp.rs`)

| Test                                      | What it verifies                                                             |
| ----------------------------------------- | ---------------------------------------------------------------------------- |
| `test_will_rename_incoming`               | `workspace/willRenameFiles` returns edits for all incoming links             |
| `test_will_rename_outgoing`               | `workspace/willRenameFiles` returns edits for outgoing links in renamed file |
| `test_will_rename_no_links`               | Renaming an unlinked file returns an empty `WorkspaceEdit`                   |
| `test_attachment_no_diagnostic`           | Link to a non-Markdown file in the workspace produces no diagnostic          |
| `test_attachment_added_clears_diagnostic` | `didChangeWatchedFiles` Created for attachment clears broken-link warning    |
| `test_attachment_deleted_adds_diagnostic` | `didChangeWatchedFiles` Deleted for attachment introduces a new warning      |
| `test_completion_includes_attachment`     | Completion at `](` returns items for non-Markdown workspace files            |
