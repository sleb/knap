# v0.4 Design — Code Actions

Covers the stories in the v0.4 release:

| Story | Feature                                                                       |
| ----- | ----------------------------------------------------------------------------- |
| US-18 | Code action: create the missing file from a broken link                       |
| US-29 | Code action: fix a broken anchor by picking from the target note's headings   |
| US-30 | Config: `newNoteDir` — notes created by Quick Fix land in a configured folder |
| US-31 | Zed extension: JSON schema for `initialization_options`                       |

---

## Goal

Fix broken links without leaving the editor. When a link points to a file that
doesn't exist, a Quick Fix creates it. When an anchor doesn't match any heading,
Quick Fix shows every available heading as a choice. Both operate through standard
`textDocument/codeAction` — no editor-specific plumbing needed.

---

## `textDocument/codeAction` Protocol

The editor sends `CodeActionParams` when the user requests a quick fix:

- `text_document` — the open file
- `range` — the cursor range (usually a collapsed range at cursor position)
- `context.diagnostics` — the diagnostics currently visible at that range

The server returns `Vec<CodeActionOrCommand>`. Each `CodeAction` carries:

- `title` — shown in the picker (e.g. "Create note", "Change anchor to \"intro\"")
- `kind` — `CodeActionKind::QUICKFIX` so editors surface it automatically
- `edit` — a `WorkspaceEdit` applied immediately when the user selects the action

Capability advertisement: `ServerCapabilities.code_action_provider = Some(Simple(true))`.

---

## Finding the Link Under the Cursor

Rather than relying on the `context.diagnostics` array (which the editor manages
and may be stale), the handler re-derives context by looking at the note's links
directly:

```
cursor = params.range.start
for link in note.md_links:
    if link.range contains cursor:
        check link status
```

`link.range` spans the full `[text](target)` construct; checking containment
here means the Quick Fix is available wherever the cursor sits on the link, not
only when it's positioned exactly over the error underline.

`link.target.is_empty()` links (anchor-only, no path) are skipped for US-18/US-29
since there is no file to create or anchor to fix in isolation.

---

## US-18 — Create Missing File

### Action

When `index.resolve(path, &link.target)` returns `Broken`, offer:

> **Create note** — `CodeActionKind::QUICKFIX`

The edit is a `WorkspaceEdit` with:

```
DocumentChanges::Operations([
    DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
        uri: path_to_uri(&new_path),
        options: Some(CreateFileOptions { ignore_if_exists: Some(true), overwrite: None }),
        annotation_id: None,
    }))
])
```

`ignore_if_exists: true` makes the action idempotent — if the user triggers it
twice, the second invocation is a no-op rather than an error.

### New file path

Two modes, controlled by `config.new_note_dir`:

**Default (no `newNoteDir`):** resolve the link target relative to the current
file's directory, then normalize. A link `[foo](../notes/foo.md)` from
`/vault/src/a.md` creates `/vault/notes/foo.md`.

```rust
normalize_path(&path.parent().unwrap().join(&link.target))
```

**With `newNoteDir`:** take only the **filename** of the link target and place it
in `{index_roots[0]}/{new_note_dir}/`. A link `[foo](../notes/foo.md)` with
`newNoteDir = "0-Inbox"` creates `/vault/0-Inbox/foo.md` regardless of where
the linking note lives.

```rust
config.index_roots.first()
    .map(|root| root.join(&config.new_note_dir.as_ref().unwrap())
                    .join(Path::new(&link.target).file_name().unwrap()))
```

If `index_roots` is empty, fall back to the default (relative) mode.

---

## US-29 — Fix Broken Anchor

### Condition

`index.resolve(path, &link.target)` returns `Found(target_path)`, the link has
an anchor (`link.anchor.is_some()`), but `slug(anchor)` doesn't match any
heading's slug in the target note.

### Actions

One action per heading in the target note:

> **Change anchor to "Introduction"** — `CodeActionKind::QUICKFIX`

The edit replaces the anchor range:

```rust
TextEdit {
    range: link.anchor_range.unwrap(),   // range of existing anchor text (no `#`)
    new_text: slug(&heading.text),
}
```

`link.anchor_range` is `None` only when the anchor wasn't present in the source
(which can't happen if `link.anchor.is_some()`), so the unwrap is safe. Actions
are skipped for any link where `anchor_range` is absent as a belt-and-suspenders
guard.

If the target note has no headings, no actions are offered — the correct fix is
to remove the anchor, which is a manual edit.

---

## US-30 — `newNoteDir` Config

### Wire-up

`InitOptions` gains:

```rust
#[serde(rename_all = "camelCase", default)]
struct InitOptions {
    extensions: Option<Vec<String>>,
    new_note_dir: Option<String>,          // ← new
}
```

`Config` stores the resolved path:

```rust
struct Config {
    index_roots:  Vec<PathBuf>,
    extensions:   Vec<String>,
    new_note_dir: Option<String>,          // ← new; relative to index_roots[0]
}
```

### Threading config into request dispatch

`dispatch_request` gains a `config: &Config` parameter. The call site in `run`
passes `&config`. `handle_code_actions` receives both `index` and `config`.
No other handlers need config today; they keep their existing signatures.

---

## US-31 — JSON Schema for Zed

A `schemas/initialization_options.json` file in the repo root documents the
`initializationOptions` shape that Zed users put in `settings.json`:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "knap initializationOptions",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "extensions": {
      "type": "array",
      "items": { "type": "string" },
      "default": ["md"],
      "description": "File extensions treated as notes (default: [\"md\"])"
    },
    "newNoteDir": {
      "type": "string",
      "description": "Folder path relative to workspace root where Quick Fix 'Create note' places new files. When absent, files are created next to the linking note."
    }
  }
}
```

Zed users reference this from their `settings.json`:

```json
"lsp": {
  "knap": {
    "initialization_options": {
      "$schema": "file:///path/to/knap/schemas/initialization_options.json",
      "extensions": ["md", "txt"]
    }
  }
}
```

The schema does not affect the server binary. It is a static file distributed
alongside the source and referenced in `GETTING_STARTED.md`.

---

## What Is Not in This Release

- **Initial file content**: created files are empty. Templates are v0.13 (US-42).
- **Anchor-only links**: `[jump](#section)` broken anchors are not offered a
  Quick Fix here. The target is the same file; rename (v0.3) already handles
  that workflow.
- **"Remove anchor" action**: if no headings exist in the target, no action is
  offered. A manual edit is the correct fix.
