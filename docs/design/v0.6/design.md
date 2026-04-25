# v0.6 Design — Code Actions

Covers the stories in the v0.6 release:

| Story | Feature                                                                     |
| ----- | --------------------------------------------------------------------------- |
| US-18 | Code action: create missing file from broken `[[link]]`                     |
| US-29 | Code action: fix broken anchor by picking from available headings           |
| US-30 | Config: `newNoteDir` — new notes from Quick Fix land in a configured folder |

**Out of scope for v0.6:** rename-via-code-action, extract-selection-to-new-note
(US-19), code actions for ambiguous links, code actions outside of wiki-link
diagnostics. The `codeAction/resolve` lazy-resolution protocol is not used —
both actions are cheap enough to compute eagerly.

---

## LSP Code Action Protocol

The client sends `textDocument/codeAction` when the user invokes the lightbulb
or Quick Fix command. The request carries a `range` (typically the cursor
position or the selected text) and a `CodeActionContext` that includes any
diagnostics overlapping the range.

The server returns `Vec<CodeAction>`. Each `CodeAction` has:

- `title` — displayed in the picker
- `kind` — `quickfix` for both of our actions
- `edit` — a `WorkspaceEdit` applied immediately when the user selects the action

No `command` field is used. Both actions are pure edits.

---

## Handler

```rust
pub fn handle_code_action(
    params: CodeActionParams,
    index: &NoteIndex,
) -> Vec<CodeAction>
```

Algorithm:

1. Resolve `params.text_document.uri` → `path` via `uri_to_path`. Return `vec![]`
   on non-file URIs.
2. Get the note at `path` from the index. Return `vec![]` if not found.
3. Find the wiki-link at `params.range.start` using `find_link_at_position`.
   Return `vec![]` if no wiki-link is at the cursor.
4. Resolve the wiki-link:
   - `Broken` → produce the **create file** action (US-18).
   - `Found(target_path)` with `link.anchor.is_some()` → check if the anchor
     matches any heading in the target note; if not, produce **anchor fix**
     actions (US-29).
   - Anything else (resolved with no anchor, ambiguous) → return `vec![]`.

Using `resolve()` to determine action eligibility is cleaner than parsing
diagnostic message strings. The result is identical since both the diagnostic
and the action code check resolution.

---

## US-18 — Create missing file

### Trigger

The wiki-link at the cursor resolves to `Broken` (no file with that stem or
filename exists in the index).

### Action produced

```
title: "Create note 'stem.md'"
kind:  quickfix
edit:  WorkspaceEdit { document_changes: [CreateFile { uri: new_uri }] }
```

### New file path

By default the new file is created in the **same directory as the current
file**. When `newNoteDir` is configured, it is created in that folder instead.

```
# default
new_path = current_file_path.parent() / (stem + "." + ext)

# with newNoteDir configured
new_path = workspace_root / newNoteDir / (stem + "." + ext)
```

`workspace_root` is the `index_root` that is an ancestor of the current note's
path. If no root matches (shouldn't occur in normal use), falls back to the
same-directory behaviour.

Extension is inferred from the current note's own extension (`.md`, `.mdx`, …);
defaults to `"md"` if absent.

`handle_code_action` receives `new_note_dir: Option<&Path>` (already resolved
to an absolute path by `dispatch_request`) rather than the raw relative string.
This keeps the handler a pure function.

```rust
let dir = new_note_dir
    .unwrap_or_else(|| path.parent().unwrap_or(Path::new("")));
let new_path = dir.join(format!("{stem}.{ext}"));
```

### Action title

When `newNoteDir` is configured the title includes the destination folder so
the user can see at a glance where the note will land:

```
# default
"Create note 'stem.md'"

# with newNoteDir = "0-Inbox"
"Create note '0-Inbox/stem.md'"
```

### WorkspaceEdit

```rust
WorkspaceEdit {
    document_changes: Some(DocumentChanges::Operations(vec![
        DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
            uri: new_uri,
            options: Some(CreateFileOptions {
                overwrite: Some(false),
                ignore_if_exists: Some(true),
            }),
            annotation_id: None,
        })),
    ])),
    ..Default::default()
}
```

`ignore_if_exists: true` makes the action idempotent — if the file was created
by another means between the diagnostic appearing and the user invoking the
action, the edit is a no-op.

The created file is empty. Adding a `# Heading` stub would require a
`TextDocumentEdit` after the `Create`, which not all clients handle correctly.
Leaving it empty is simpler and more compatible.

---

## US-29 — Fix broken anchor

### Trigger

The wiki-link at the cursor resolves to `Found(target_path)` and
`link.anchor` is `Some(anchor)` where `anchor` does not match any heading in
the target note (case-insensitive). This is exactly the condition that emits a
"Heading not found" diagnostic.

### Actions produced

One `CodeAction` per heading in the target note:

```
title: "Change anchor to '#HeadingText'"
kind:  quickfix
edit:  WorkspaceEdit replacing link.anchor_range with heading.text
```

If the target note has no headings, no actions are returned.

### WorkspaceEdit

```rust
WorkspaceEdit {
    changes: Some(HashMap::from([(
        path_to_uri(&path),
        vec![TextEdit {
            range: link.anchor_range.expect("anchor exists"),
            new_text: heading.text.clone(),
        }],
    )])),
    ..Default::default()
}
```

`link.anchor_range` covers only the anchor text (not the `#`), so replacing it
with the new heading text produces `[[stem#NewHeading]]` correctly.

If the target note has many headings, the list may be long. The editor's Quick
Fix picker is the right UI for choosing — no special filtering is needed here.

---

## Capability advertisement

Add `code_action_provider` to `ServerCapabilities`:

```rust
code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
    code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
    resolve_provider: Some(false),
    ..Default::default()
})),
```

Add routing in `dispatch_request`:

```rust
"textDocument/codeAction" => {
    let actions = serde_json::from_value::<CodeActionParams>(req.params)
        .ok()
        .map(|params| handlers::handle_code_action(params, index, new_note_dir))
        .unwrap_or_default();
    connection.sender.send(Message::Response(Response::new_ok(req.id, actions)))?;
}
```

---

## US-30 — `newNoteDir` configuration

### Config key

`initializationOptions.newNoteDir` — optional string, path relative to each
workspace root. When absent, same-directory behaviour is preserved.

```json
{ "newNoteDir": "0-Inbox" }
```

Stored in `Config` as `new_note_dir: Option<PathBuf>`. Resolved to an absolute
path in `dispatch_request` before being passed to `handle_code_action`.

---

## US-31 — Zed extension: `initialization_options` JSON schema

### Mechanism

The `zed_extension_api` trait exposes an optional
`language_server_initialization_options_schema` method. When overridden, Zed
uses the returned JSON Schema value to validate and autocomplete whatever the
user writes under the `initialization_options` key in `settings.json`.

There is no `extension.toml` key for this — it is a Rust method override in
`src/lib.rs` of the `zed-knap` extension.

### Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema",
  "type": "object",
  "properties": {
    "extensions": {
      "type": "array",
      "items": { "type": "string" },
      "description": "File extensions treated as notes (default: [\"md\"])"
    },
    "attachmentsDir": {
      "type": "string",
      "description": "Subdirectory for attachment files (e.g. \"assets\")"
    },
    "newNoteDir": {
      "type": "string",
      "description": "Folder (relative to workspace root) where Quick Fix 'Create note' actions land (e.g. \"0-Inbox\")"
    }
  },
  "additionalProperties": false
}
```

`additionalProperties: false` is what triggers the "key is not defined" warning
the editor shows for unknown keys.

### Implementation location

`zed-knap` (`src/lib.rs`), not in the knap server itself. The schema must be
kept in sync with `InitOptions` in `src/server/mod.rs` whenever a new config
key is added.

---

## Testing

### Unit tests (`src/handlers.rs` inline)

| Test                                        | What it verifies                                                                          |
| ------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `code_action_broken_link_creates_file`      | Broken `[[missing]]` → one action, title contains stem, edit creates file in same dir     |
| `code_action_broken_link_same_extension`    | Current file is `.mdx` → new file gets `.mdx` extension                                   |
| `code_action_resolved_link_no_action`       | Valid `[[found]]` → empty actions                                                         |
| `code_action_broken_anchor_lists_headings`  | `[[note#Bad]]` with `note.md` having `## Good` and `## Other` → two actions               |
| `code_action_broken_anchor_edit_range`      | Edit range targets `link.anchor_range` (anchor text only, not `#`)                        |
| `code_action_no_headings_no_anchor_actions` | `[[note#Bad]]` with target having no headings → empty actions                             |
| `code_action_no_link_at_cursor`             | Cursor not on a wiki-link → empty actions                                                 |
| `code_action_ambiguous_no_action`           | Ambiguous link → empty actions (not a supported case)                                     |
| `code_action_new_note_dir_used`             | `newNoteDir` configured → CreateFile URI uses configured dir; title shows `"dir/stem.md"` |
| `code_action_new_note_dir_fallback`         | `newNoteDir` configured but no index_root matches → falls back to same-dir                |

### Integration tests (`tests/code_actions.rs`)

| Test                            | What it verifies                                                                     |
| ------------------------------- | ------------------------------------------------------------------------------------ |
| `create_file_action_round_trip` | Full LSP round-trip: broken link → code action request → CreateFile edit returned    |
| `fix_anchor_action_round_trip`  | Full LSP round-trip: broken anchor → code action request → TextEdit replacing anchor |
| `no_action_on_valid_link`       | Valid link at cursor → empty code action response                                    |
| `new_note_dir_round_trip`       | `newNoteDir` in initializationOptions → CreateFile URI uses configured dir           |
