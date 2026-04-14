# v0.2 Design — Rename & Refactor

Covers the stories in the v0.2 release:

| Story  | Feature                                                                       |
| ------ | ----------------------------------------------------------------------------- |
| US-04  | Rename file → update all `[[links]]`                                          |
| US-05  | Aliased links `[[Note\|display text]]` — rename support; completion unchanged |
| US-07b | Diagnostic for ambiguous stems _(already implemented in v0.1)_                |
| US-21  | Config: file extensions treated as notes                                      |
| US-26  | Attachment links (`[[image.png]]`) resolve against non-md files               |

**Out of scope for v0.2:** frontmatter parsing, heading navigation, hover previews,
completion label changes (deferred to v0.3/v0.4). US-22 and US-27 have been moved
to the backlog — see `docs/ROADMAP.md`.

---

## Link syntax

v0.1 handled three syntactic forms but classified them all the same way. v0.2
adds one more form with distinct resolution semantics:

| Pattern            | Resolution                         |
| ------------------ | ---------------------------------- |
| `[[stem]]`         | by stem                            |
| `[[stem#section]]` | by stem; section stripped          |
| `[[stem\|alias]]`  | by stem; alias preserved on rename |
| `[[filename.ext]]` | by full filename                   |

There is no explicit classification step. The index `resolve` method handles all
forms transparently — see [Index changes](#index-changes) below.

`![[filename.ext]]` embed syntax is treated identically to `[[filename.ext]]` — the
parser finds `[[` regardless of the preceding `!`, so no special handling is needed.

---

## Data structures

### WikiLink (unchanged)

No structural changes to `WikiLink` in v0.2. `inner_range` already spans only the
stem, not the alias, so `[[old-stem|my alias]]` is correctly rewritten to
`[[new-stem|my alias]]` by a rename edit without ever reading the alias text.

**Note on alias storage:** the alias is not stored in v0.2 because no handler reads
it — rename correctness comes from `inner_range`. If v0.4 frontmatter alias support
needs the alias text for completion labels, add `alias: Option<String>` then.

### Config (changed)

```
Config {
  index_roots:     Vec<PathBuf>
  extensions:      Vec<String>        // default: ["md"]
  attachments_dir: Option<PathBuf>    // default: None; relative to workspace root
}
```

Read from `initializationOptions` in the `initialize` request:

```json
{
  "extensions": ["md", "mdx"],
  "attachmentsDir": "assets"
}
```

Missing fields fall back to defaults. `workspace/didChangeConfiguration` updates
`extensions` and `attachments_dir` at runtime and triggers a full re-index.

### NoteIndex (changed)

Add attachment file tracking:

```
NoteIndex {
  by_path:     Map<PathBuf, Note>             // unchanged
  by_stem:     Map<String, Vec<PathBuf>>      // note links: stem → paths
  by_filename: Map<String, Vec<PathBuf>>      // attachment links: full filename → paths
  links_to:    Map<PathBuf, Vec<LocatedLink>> // unchanged; covers note and attachment links
}
```

`by_filename` is keyed by the full filename including extension (e.g. `"diagram.png"`).
It covers all files in the workspace, not just configured note extensions.

---

## Parser changes

No changes to the parser in v0.2. It already correctly extracts the stem from all
link forms (`[[stem]]`, `[[stem#section]]`, `[[stem|alias]]`). Attachment and note
links are distinguished at resolution time, not parse time.

---

## Index changes

### Startup crawl

The initial `build()` crawl walks **all files** under each workspace root:

- **Note files** (matching `config.extensions`): parse and add to `by_path`, `by_stem`,
  and `by_filename`.
- **All other files**: add their full filename to `by_filename` only (no parsing).

### Attachment file watching

If `attachments_dir` is set, register a second file watcher at `initialized` using
a `RelativePattern` scoped to that directory:

```
RelativePattern {
  base:    workspace_root_uri / attachments_dir,
  pattern: "**/*",
}
```

Events from this watcher arrive via the existing `workspace/didChangeWatchedFiles`
handler. The handler checks whether the changed file's extension is a configured note
extension:

- **Note extension** → re-parse and fully re-index (existing behavior).
- **Any other extension** → update `by_filename` only: add on `Created`, remove on
  `Deleted`, no-op on `Changed`.

If `attachments_dir` is **not** set, `by_filename` is populated once at startup and
not updated incrementally. Attachment link diagnostics may be stale if non-note files
are added or deleted while the server is running; they correct at the next restart.

### `index(note: Note)` (changed)

After the existing `by_stem` update, also register the note in `by_filename`:

```
self.by_filename.entry(note.filename()).or_default().push(note.path.clone());
```

where `note.filename()` returns the full filename including extension (e.g. `"my-note.md"`).

For `links_to`: resolve each wiki-link via `resolve` (which now covers both notes and
attachments) and populate the reverse index as before.

### `remove(path)` (changed)

Also remove from `by_filename`.

### `resolve(target: &str) → ResolvedLink` (changed)

The existing `resolve` is extended to fall through to `by_filename` when `by_stem`
finds nothing:

```
look up by_stem[target]:
  len 1    → Found
  len > 1  → Ambiguous
  missing  → look up by_filename[target]:
               len 1    → Found
               len > 1  → Ambiguous
               missing  → Broken
```

This handles all link forms without any classification step. `[[image.png]]` fails
`by_stem` (no note has stem `image.png`) and succeeds via `by_filename`. `[[note]]`
finds the note in `by_stem` without ever touching `by_filename`. A note named
`my.note` resolves correctly via `by_stem` — the dot in the stem is not
misinterpreted. No callers change.

---

## LSP handlers

### Rename (`workspace/willRenameFiles`) — new

Advertise capability during `initialize`:

```rust
capabilities.rename_provider = Some(OneOf::Right(RenameOptions {
    prepare_provider: Some(false),
    work_done_progress_options: Default::default(),
}));
```

Also register `workspace/willRenameFiles` so the client sends pre-rename requests:

```json
{ "filters": [{ "scheme": "file" }] }
```

**Handler algorithm:**

```
handle_will_rename_files(params, index) → WorkspaceEdit:
  changes: Map<Uri, Vec<TextEdit>> = {}

  for each rename in params.files:
    old_path = uri_to_path(rename.old_uri)
    new_stem = path_to_stem(rename.new_uri)  // filename without extension

    for each link in index.links_to(old_path):
      edit = TextEdit {
        range:    link.wiki_link.inner_range,
        new_text: new_stem,
      }
      changes[path_to_uri(link.source_path)].push(edit)

  return WorkspaceEdit { changes }
```

The editor applies the `WorkspaceEdit` before performing the rename, then sends
`textDocument/didChange` for affected files and `workspace/didChangeWatchedFiles`
for the renamed file itself. The index updates via existing notification handlers —
no special rename handling is needed there.

**Alias preservation:** because `inner_range` spans only the stem, not the alias,
`[[old-stem|my alias]]` is correctly rewritten to `[[new-stem|my alias]]`.

### Completion (`textDocument/completion`) — unchanged

No changes in v0.2. Frontmatter alias labels are deferred to v0.4.

### Diagnostics (`textDocument/publishDiagnostics`) — updated

`compute_diagnostics` calls `resolve(&link.stem)` for every link, unchanged in
structure. Only the messages change to be link-agnostic:

| Resolution  | Severity | Message                                              |
| ----------- | -------- | ---------------------------------------------------- |
| `Broken`    | Warning  | `Link target not found: '[[stem]]'`                  |
| `Ambiguous` | Warning  | `'[[stem]]' matches multiple files: path1, path2, …` |
| `Found`     | —        | no diagnostic                                        |

The previous messages (`No note found for`, `matches multiple notes`) are replaced
to cover attachment links naturally without special-casing.

---

## Startup sequence (updated)

```
initialize received
  → parse workspaceFolders + initializationOptions into Config
    (extensions: ["md"] default; attachments_dir: None default)
  → respond to initialize with capabilities
    (including willRenameFiles)

initialized received
  → register note file watcher: one glob per configured extension ("**/*.md", etc.)
  → if attachments_dir set: register RelativePattern watcher for "**/*" under that dir
  → crawl all files under workspace roots:
      note files   → parse → index (by_path, by_stem, by_filename)
      other files  → register filename in by_filename only
  → publish initial diagnostics for all broken/ambiguous links
```

---

## Testing

### Unit tests

- Parser: aliased links produce `inner_range` spanning only the stem, not the alias
- Index: `resolve` returns `Found` for a note stem in `by_stem`
- Index: `resolve` falls through to `by_filename` when stem not in `by_stem`
- Index: `resolve` returns `Broken` when target is in neither map
- Index: `index(note)` populates `by_filename` for note files
- Index: non-note files registered in `by_filename` are found via `resolve`
- Handler: `handle_will_rename_files` produces correct edits for plain links
- Handler: `handle_will_rename_files` preserves alias — `[[old|alias]]` → `[[new|alias]]`
- Handler: renamed file with no backlinks returns an empty `WorkspaceEdit`
- Diagnostics: broken attachment link produces `Link target not found: '[[…]]'`
- Diagnostics: attachment link present in workspace produces no diagnostic

### Integration tests

- Full rename flow: rename a file, verify the `WorkspaceEdit` rewrite; simulate
  `didChange` for the affected file and verify the index reflects the new stem
- Attachment link: index a note with `[[image.png]]`; absent → diagnostic;
  present (populated at startup) → no diagnostic
